//! Socket protocol and collaboration transport.
//!
//! Message layouts mirror the server protocol and the socketio error type is
//! fixed by the external crate.
#![allow(clippy::large_enum_variant)]
#![allow(clippy::result_large_err)]

use std::path::PathBuf;
use std::sync::{mpsc, Mutex};
use std::thread;

use rust_socketio::{ClientBuilder, Event, Payload, RawClient};

use crate::packet::Packet;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NetworkMember {
    pub id: String,
    pub username: String,
    pub role: String,
    #[serde(default)]
    pub muted: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RecordingPreparePayload {
    pub project: crate::recording::RecordingProject,
    pub transactions: crate::recording::TransactionLog,
    pub current_frame: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capture_target: Option<crate::recording::CaptureTarget>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RecordingPlaybackPayload {
    pub frame: i64,
    pub playing: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RecordingCapturePayload {
    pub current_frame: i64,
    pub capture_target: Option<crate::recording::CaptureTarget>,
}

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
    RoomMetadata {
        member_id: String,
        project_huuid: String,
    },
    RoomState {
        members: Vec<NetworkMember>,
        control_owner_id: Option<String>,
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
    AudioStart {
        metadata: serde_json::Value,
    },
    AudioChunk {
        transfer_id: String,
        index: usize,
        data_base64: String,
    },
    AudioEnd {
        transfer_id: String,
    },
    RecordingTransaction(crate::recording::RecordingTransaction),
    RecordingPrepare(RecordingPreparePayload),
    RecordingCapture(RecordingCapturePayload),
    RecordingPlayback(RecordingPlaybackPayload),
}

/// Outgoing message: event name + JSON payload, sent via dedicated sender thread.
struct OutgoingMessage(String, serde_json::Value);

pub struct NetworkClient {
    _client: Option<rust_socketio::client::Client>,
    out_tx: Option<mpsc::SyncSender<OutgoingMessage>>,
    rx: Option<mpsc::Receiver<IncomingMessage>>,
    pub state: ConnectionState,
    pub room_code: Option<String>,
    pub role: Option<String>,
    pub members: Vec<String>,
    pub member_id: Option<String>,
    pub project_huuid: Option<String>,
    pub member_details: Vec<NetworkMember>,
    pub control_owner_id: Option<String>,
}

impl Default for NetworkClient {
    fn default() -> Self {
        Self::new()
    }
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
            member_id: None,
            project_huuid: None,
            member_details: Vec::new(),
            control_owner_id: None,
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
        // Bound queued payloads so a multi-gigabyte take cannot be expanded
        // to base64 in memory faster than Socket.IO can emit it.
        let (out_tx, out_rx) = mpsc::sync_channel::<OutgoingMessage>(32);
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
        let tx_room_metadata_created = in_tx.clone();
        let tx_room_metadata_joined = in_tx.clone();
        let tx_room_state = in_tx.clone();
        let tx_audio_start = in_tx.clone();
        let tx_audio_chunk = in_tx.clone();
        let tx_audio_end = in_tx.clone();
        let tx_recording_transaction = in_tx.clone();
        let tx_recording_prepare = in_tx.clone();
        let tx_recording_capture = in_tx.clone();
        let tx_recording_playback = in_tx.clone();

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
                let member_id = extract_string_field(&payload, "member_id");
                let project_huuid = extract_string_field(&payload, "project_huuid");
                let _ = tx_room_metadata_created.send(IncomingMessage::RoomMetadata {
                    member_id,
                    project_huuid,
                });
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
                    let member_id = obj["member_id"].as_str().unwrap_or("").to_string();
                    let project_huuid = obj["project_huuid"].as_str().unwrap_or("").to_string();
                    let _ = tx_room_metadata_joined.send(IncomingMessage::RoomMetadata {
                        member_id,
                        project_huuid,
                    });
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
            })
            .on("room_state", move |payload, _| {
                if let Some(obj) = payload_to_value(&payload) {
                    let members =
                        serde_json::from_value::<Vec<NetworkMember>>(obj["members"].clone())
                            .unwrap_or_default();
                    let control_owner_id = obj["control_owner_id"].as_str().map(String::from);
                    let _ = tx_room_state.send(IncomingMessage::RoomState {
                        members,
                        control_owner_id,
                    });
                }
            })
            .on("audio_start", move |payload, _| {
                if let Some(metadata) = payload_to_value(&payload) {
                    let _ = tx_audio_start.send(IncomingMessage::AudioStart { metadata });
                }
            })
            .on("audio_chunk", move |payload, _| {
                if let Some(obj) = payload_to_value(&payload) {
                    let transfer_id = obj["transfer_id"].as_str().unwrap_or("").to_string();
                    let index = obj["index"].as_u64().unwrap_or(0) as usize;
                    let data_base64 = obj["data"].as_str().unwrap_or("").to_string();
                    let _ = tx_audio_chunk.send(IncomingMessage::AudioChunk {
                        transfer_id,
                        index,
                        data_base64,
                    });
                }
            })
            .on("audio_end", move |payload, _| {
                if let Some(obj) = payload_to_value(&payload) {
                    let transfer_id = obj["transfer_id"].as_str().unwrap_or("").to_string();
                    let _ = tx_audio_end.send(IncomingMessage::AudioEnd { transfer_id });
                }
            })
            .on("recording_transaction", move |payload, _| {
                if let Some(value) = payload_to_value(&payload) {
                    match serde_json::from_value(value) {
                        Ok(transaction) => {
                            let _ = tx_recording_transaction
                                .send(IncomingMessage::RecordingTransaction(transaction));
                        }
                        Err(error) => {
                            let _ = tx_recording_transaction.send(IncomingMessage::Error(format!(
                                "invalid recording transaction: {error}"
                            )));
                        }
                    }
                }
            })
            .on("recording_prepare", move |payload, _| {
                if let Some(value) = payload_to_value(&payload) {
                    match serde_json::from_value(value) {
                        Ok(prepare) => {
                            let _ = tx_recording_prepare
                                .send(IncomingMessage::RecordingPrepare(prepare));
                        }
                        Err(error) => {
                            let _ = tx_recording_prepare.send(IncomingMessage::Error(format!(
                                "invalid recording preparation: {error}"
                            )));
                        }
                    }
                }
            })
            .on("recording_capture", move |payload, _| {
                if let Some(value) = payload_to_value(&payload) {
                    match serde_json::from_value(value) {
                        Ok(capture) => {
                            let _ = tx_recording_capture
                                .send(IncomingMessage::RecordingCapture(capture));
                        }
                        Err(error) => {
                            let _ = tx_recording_capture.send(IncomingMessage::Error(format!(
                                "invalid recording capture command: {error}"
                            )));
                        }
                    }
                }
            })
            .on("recording_playback", move |payload, _| {
                if let Some(value) = payload_to_value(&payload) {
                    match serde_json::from_value(value) {
                        Ok(playback) => {
                            let _ = tx_recording_playback
                                .send(IncomingMessage::RecordingPlayback(playback));
                        }
                        Err(error) => {
                            let _ = tx_recording_playback.send(IncomingMessage::Error(format!(
                                "invalid recording playback command: {error}"
                            )));
                        }
                    }
                }
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

    pub fn send_recording_transaction(&self, transaction: &crate::recording::RecordingTransaction) {
        if let Ok(payload) = serde_json::to_value(transaction) {
            self.send_raw("recording_transaction", payload);
        }
    }

    pub fn send_recording_prepare(&self, prepare: &RecordingPreparePayload) {
        if let Ok(payload) = serde_json::to_value(prepare) {
            self.send_raw("recording_prepare", payload);
        }
    }

    pub fn send_recording_prepare_to(&self, prepare: &RecordingPreparePayload, member_id: &str) {
        if let Ok(mut payload) = serde_json::to_value(prepare) {
            payload["_target"] = serde_json::Value::String(member_id.to_owned());
            self.send_raw("recording_prepare", payload);
        }
    }

    pub fn send_recording_capture(
        &self,
        current_frame: i64,
        capture_target: Option<crate::recording::CaptureTarget>,
    ) {
        let capture = RecordingCapturePayload {
            current_frame,
            capture_target,
        };
        if let Ok(payload) = serde_json::to_value(capture) {
            self.send_raw("recording_capture", payload);
        }
    }

    pub fn send_recording_playback(&self, frame: i64, playing: bool) {
        let payload = RecordingPlaybackPayload { frame, playing };
        if let Ok(payload) = serde_json::to_value(payload) {
            self.send_raw("recording_playback", payload);
        }
    }

    pub fn set_co_director(&self, member_id: &str, enabled: bool) {
        self.send_raw(
            "set_co_director",
            serde_json::json!({ "member_id": member_id, "enabled": enabled }),
        );
    }

    pub fn grant_recording_control(&self, member_id: &str) {
        self.send_raw(
            "grant_recording_control",
            serde_json::json!({ "member_id": member_id }),
        );
    }

    pub fn set_member_muted(&self, member_id: &str, muted: bool) {
        self.send_raw(
            "set_member_muted",
            serde_json::json!({ "member_id": member_id, "muted": muted }),
        );
    }

    pub fn kick_member(&self, member_id: &str) {
        self.send_raw("kick_member", serde_json::json!({ "member_id": member_id }));
    }

    pub fn ban_member_ip(&self, member_id: &str) {
        self.send_raw(
            "ban_member_ip",
            serde_json::json!({ "member_id": member_id }),
        );
    }

    /// Stream one FLAC from a worker through the bounded sender queue.
    pub fn send_audio_file(
        &self,
        path: PathBuf,
        metadata: crate::audio_transfer::AudioTransferMetadata,
    ) -> mpsc::Receiver<Result<(), String>> {
        let (result_tx, result_rx) = mpsc::channel();
        let Some(out_tx) = self.out_tx.clone() else {
            let _ = result_tx.send(Err("network is not connected".into()));
            return result_rx;
        };
        let spawn_error_tx = result_tx.clone();
        if let Err(error) = thread::Builder::new()
            .name("recording-audio-upload".into())
            .spawn(move || {
                let result = (|| {
                    metadata.validate()?;
                    let start = serde_json::to_value(&metadata)
                        .map_err(|error| format!("cannot serialize FLAC metadata: {error}"))?;
                    out_tx
                        .send(OutgoingMessage("audio_start".into(), start))
                        .map_err(|_| "network sender stopped".to_string())?;
                    let reader = crate::audio_transfer::AudioChunkReader::open(&path, &metadata)?;
                    for chunk in reader {
                        let chunk = chunk?;
                        out_tx
                            .send(OutgoingMessage(
                                "audio_chunk".into(),
                                serde_json::json!({
                                    "transfer_id": &metadata.transfer_id,
                                    "index": chunk.index,
                                    "data": chunk.data_base64,
                                }),
                            ))
                            .map_err(|_| "network sender stopped".to_string())?;
                    }
                    out_tx
                        .send(OutgoingMessage(
                            "audio_end".into(),
                            serde_json::json!({ "transfer_id": &metadata.transfer_id }),
                        ))
                        .map_err(|_| "network sender stopped".to_string())?;
                    Ok(())
                })();
                let _ = result_tx.send(result);
            })
        {
            let _ = spawn_error_tx.send(Err(format!("cannot start FLAC upload: {error}")));
        }
        result_rx
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
        self.member_id = None;
        self.project_huuid = None;
        self.member_details.clear();
        self.control_owner_id = None;
    }
}

impl Drop for NetworkClient {
    fn drop(&mut self) {
        self.disconnect();
    }
}

fn packet_to_emit(packet: &Packet) -> (&'static str, serde_json::Value) {
    match packet {
        Packet::CreateRoom {
            username,
            project_huuid,
        } => (
            "create_room",
            serde_json::json!({ "username": username, "project_huuid": project_huuid }),
        ),
        Packet::JoinRoom {
            code,
            username,
            project_huuid,
        } => (
            "join_room",
            serde_json::json!({
                "code": code,
                "username": username,
                "project_huuid": project_huuid,
            }),
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
