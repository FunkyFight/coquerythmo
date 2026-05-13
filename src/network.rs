use std::sync::{mpsc, Mutex};
use std::thread;

use rust_socketio::{ClientBuilder, Event, Payload, RawClient};

use crate::packet::Packet;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    InRoom,
}

pub enum IncomingMessage {
    Packet(Packet),
    Connected,
    Disconnected(String),
    Error(String),
    SyncRequested {
        requester: String,
    },
    Delta(serde_json::Value),
    VideoStart {
        filename: String,
        total_chunks: usize,
    },
    VideoChunk {
        index: usize,
        data_base64: String,
    },
    VideoEnd,
}

/// Outgoing message: event name + JSON payload, sent via dedicated sender thread.
struct OutgoingMessage(String, serde_json::Value);

pub struct NetworkClient {
    _client: Option<rust_socketio::client::Client>,
    out_tx: Option<mpsc::Sender<OutgoingMessage>>,
    rx: Option<mpsc::Receiver<IncomingMessage>>,
    pub state: ConnectionState,
    pub room_code: Option<String>,
    pub role: Option<String>,
    pub members: Vec<String>,
}

impl NetworkClient {
    pub fn new() -> Self {
        Self {
            _client: None,
            out_tx: None,
            rx: None,
            state: ConnectionState::Disconnected,
            room_code: None,
            role: None,
            members: Vec::new(),
        }
    }

    pub fn is_in_room(&self) -> bool {
        self.state == ConnectionState::InRoom
    }

    pub fn is_connected(&self) -> bool {
        matches!(
            self.state,
            ConnectionState::Connected | ConnectionState::InRoom
        )
    }

    pub fn connect_and_send(&mut self, ip: &str, port: u16, password: &str, first_packet: Packet) {
        if self.state != ConnectionState::Disconnected {
            self.disconnect();
        }
        self.state = ConnectionState::Connecting;

        let (in_tx, in_rx) = mpsc::channel::<IncomingMessage>();
        let (out_tx, out_rx) = mpsc::channel::<OutgoingMessage>();
        let url = format!("http://{}:{}", ip, port);
        log::info!("Connecting to {url}");

        let tx_connect = in_tx.clone();
        let tx_disconnect = in_tx.clone();
        let tx_room_created = in_tx.clone();
        let tx_room_joined = in_tx.clone();
        let tx_join_error = in_tx.clone();
        let tx_member_joined = in_tx.clone();
        let tx_member_left = in_tx.clone();
        let tx_remote_command = in_tx.clone();
        let tx_sync = in_tx.clone();
        let tx_request_sync = in_tx.clone();
        let tx_delta = in_tx.clone();
        let tx_error = in_tx.clone();
        let tx_vstart = in_tx.clone();
        let tx_vchunk = in_tx.clone();
        let tx_vend = in_tx.clone();

        let (first_event, first_payload) = packet_to_emit(&first_packet);
        let first_event = first_event.to_string();
        let out_rx = Mutex::new(Some(out_rx));

        let builder = ClientBuilder::new(&url)
            .auth(serde_json::json!({ "password": password }))
            .on(Event::Connect, move |_, client: RawClient| {
                let _ = tx_connect.send(IncomingMessage::Connected);
                let _ = client.emit(&*first_event, first_payload.clone());
                // Take out_rx once and spawn sender thread
                if let Some(rx) = out_rx.lock().unwrap().take() {
                    let raw = client;
                    thread::spawn(move || {
                        while let Ok(OutgoingMessage(event, payload)) = rx.recv() {
                            let _ = raw.emit(&*event, payload);
                        }
                    });
                }
            })
            .on(Event::Error, move |err, _| {
                let msg = match &err {
                    Payload::Text(v) => v
                        .first()
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown error")
                        .to_string(),
                    _ => "unknown error".into(),
                };
                let _ = tx_disconnect.send(IncomingMessage::Error(msg));
            })
            .on("room_created", move |payload, _| {
                let code = extract_string_field(&payload, "code");
                let _ = tx_room_created.send(IncomingMessage::Packet(Packet::RoomCreated { code }));
            })
            .on("room_joined", move |payload, client: RawClient| {
                if let Some(obj) = payload_to_value(&payload) {
                    let code = obj["code"].as_str().unwrap_or("").to_string();
                    let role = obj["role"].as_str().unwrap_or("user").to_string();
                    let members = obj["members"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default();
                    let _ = tx_room_joined.send(IncomingMessage::Packet(Packet::RoomJoined {
                        code,
                        role,
                        members,
                    }));
                    // Request sync directly from callback
                    let _ = client.emit("request_sync", serde_json::json!({}));
                }
            })
            .on("join_error", move |payload, _| {
                let reason = extract_string_field(&payload, "reason");
                let _ = tx_join_error.send(IncomingMessage::Packet(Packet::JoinError { reason }));
            })
            .on("member_joined", move |payload, _| {
                let username = extract_string_field(&payload, "username");
                let _ = tx_member_joined
                    .send(IncomingMessage::Packet(Packet::MemberJoined { username }));
            })
            .on("member_left", move |payload, _| {
                let username = extract_string_field(&payload, "username");
                let _ =
                    tx_member_left.send(IncomingMessage::Packet(Packet::MemberLeft { username }));
            })
            .on("remote_command", move |payload, _| {
                let obj = payload_to_value(&payload);
                if obj.is_none() {
                    let _ = tx_remote_command
                        .send(IncomingMessage::Error("remote_command: no payload".into()));
                    return;
                }
                let obj = obj.unwrap();
                let from = obj["from"].as_str().unwrap_or("?").to_string();
                let raw_payload = obj["payload"].clone();
                match serde_json::from_value::<crate::packet::CommandPayload>(raw_payload.clone()) {
                    Ok(cmd_payload) => {
                        let _ = tx_remote_command.send(IncomingMessage::Packet(
                            Packet::RemoteCommand {
                                from,
                                payload: cmd_payload,
                            },
                        ));
                    }
                    Err(e) => {
                        let _ = tx_remote_command
                            .send(IncomingMessage::Error(format!("Déser. échouée: {e}")));
                    }
                }
            })
            .on("sync", move |payload, _| {
                if let Some(obj) = payload_to_value(&payload) {
                    if let Ok(project) = serde_json::from_value(obj["project"].clone()) {
                        let _ = tx_sync.send(IncomingMessage::Packet(Packet::Sync { project }));
                    }
                }
            })
            .on("request_sync", move |payload, _| {
                let requester = payload_to_value(&payload)
                    .and_then(|v| v["requester"].as_str().map(String::from))
                    .unwrap_or_default();
                let _ = tx_request_sync.send(IncomingMessage::SyncRequested { requester });
            })
            .on("server_error", move |payload, _| {
                let message = extract_string_field(&payload, "message");
                let _ = tx_error.send(IncomingMessage::Packet(Packet::Error { message }));
            })
            .on("delta", move |payload, _| {
                if let Some(obj) = payload_to_value(&payload) {
                    let _ = tx_delta.send(IncomingMessage::Delta(obj));
                }
            })
            .on("video_start", move |payload, _| {
                if let Some(obj) = payload_to_value(&payload) {
                    let filename = obj["filename"].as_str().unwrap_or("video.mp4").to_string();
                    let total_chunks = obj["total_chunks"].as_u64().unwrap_or(0) as usize;
                    let _ = tx_vstart.send(IncomingMessage::VideoStart {
                        filename,
                        total_chunks,
                    });
                }
            })
            .on("video_chunk", move |payload, _| {
                if let Some(obj) = payload_to_value(&payload) {
                    let index = obj["index"].as_u64().unwrap_or(0) as usize;
                    let data_base64 = obj["data"].as_str().unwrap_or("").to_string();
                    let _ = tx_vchunk.send(IncomingMessage::VideoChunk { index, data_base64 });
                }
            })
            .on("video_end", move |_, _| {
                let _ = tx_vend.send(IncomingMessage::VideoEnd);
            });

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| builder.connect()));
        let result = match result {
            Ok(Ok(client)) => Ok(client),
            Ok(Err(e)) => Err(format!("{e}")),
            Err(_) => Err("Connexion échouée (panic)".into()),
        };

        match result {
            Ok(client) => {
                self._client = Some(client);
                self.out_tx = Some(out_tx);
                self.rx = Some(in_rx);
            }
            Err(e) => {
                log::error!("Socket.io connection failed: {e}");
                let _ = in_tx.send(IncomingMessage::Error(format!("Connexion échouée: {e}")));
                self.rx = Some(in_rx);
                self.state = ConnectionState::Disconnected;
            }
        }
    }

    /// Send a packet via the sender thread.
    pub fn send(&self, packet: &Packet) {
        let (event, payload) = packet_to_emit(packet);
        self.send_raw(event, payload);
    }

    /// Send a raw event via the sender thread.
    pub fn send_raw(&self, event: &str, payload: serde_json::Value) {
        log::debug!("Sending event: {event}");
        if let Some(tx) = &self.out_tx {
            let _ = tx.send(OutgoingMessage(event.to_string(), payload));
        }
    }

    pub fn try_recv(&self) -> Option<IncomingMessage> {
        self.rx.as_ref()?.try_recv().ok()
    }

    pub fn disconnect(&mut self) {
        log::info!("Disconnecting from server");
        // Drop out_tx first to stop sender thread
        self.out_tx = None;
        if let Some(client) = self._client.take() {
            let _ = client.disconnect();
        }
        self.rx = None;
        self.state = ConnectionState::Disconnected;
        self.room_code = None;
        self.role = None;
        self.members.clear();
    }
}

impl Drop for NetworkClient {
    fn drop(&mut self) {
        self.disconnect();
    }
}

fn packet_to_emit(packet: &Packet) -> (&'static str, serde_json::Value) {
    match packet {
        Packet::CreateRoom { username } => {
            ("create_room", serde_json::json!({ "username": username }))
        }
        Packet::JoinRoom { code, username } => (
            "join_room",
            serde_json::json!({ "code": code, "username": username }),
        ),
        Packet::LeaveRoom => ("leave_room", serde_json::json!({})),
        Packet::Command { payload } => ("command", serde_json::json!({ "payload": payload })),
        Packet::RequestSync => ("request_sync", serde_json::json!({})),
        Packet::Sync { project } => ("sync", serde_json::json!({ "project": project })),
        _ => ("unknown", serde_json::json!({})),
    }
}

fn payload_to_value(payload: &Payload) -> Option<serde_json::Value> {
    match payload {
        Payload::Text(values) => values.first().cloned(),
        _ => None,
    }
}

fn extract_string_field(payload: &Payload, field: &str) -> String {
    payload_to_value(payload)
        .and_then(|v| v[field].as_str().map(String::from))
        .unwrap_or_default()
}
