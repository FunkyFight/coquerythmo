//! Socket protocol and collaboration transport.
//!
//! Message layouts mirror the server protocol and the socketio error type is
//! fixed by the external crate.
#![allow(clippy::large_enum_variant)]
#![allow(clippy::result_large_err)]

use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc, Mutex,
};
use std::thread;
use std::time::Duration;

use rust_socketio::{ClientBuilder, Event, Payload, RawClient};

use crate::packet::Packet;

#[cfg(test)]
mod integration_tests;
pub(crate) mod replication;
mod rpc;
pub(crate) mod rythmo;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NetworkMember {
    pub id: String,
    pub username: String,
    pub role: String,
    #[serde(default)]
    pub muted: bool,
    #[serde(default)]
    pub recording_ready: bool,
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
pub struct RecordingViewPayload {
    pub language_id: u64,
    pub instrumental: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instrumental_audio_offset_frames: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RecordingCapturePayload {
    pub current_frame: i64,
    pub capture_target: Option<crate::recording::CaptureTarget>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProjectTransferMetadata {
    pub request_id: String,
    pub project_huuid: String,
    pub file_name: String,
    pub total_bytes: u64,
    pub total_chunks: usize,
    pub chunk_size: usize,
    pub sha1: String,
}

impl ProjectTransferMetadata {
    fn file_metadata(&self) -> crate::file_transfer::FileTransferMetadata {
        crate::file_transfer::FileTransferMetadata {
            transfer_id: self.request_id.clone(),
            file_name: self.file_name.clone(),
            total_bytes: self.total_bytes,
            total_chunks: self.total_chunks,
            chunk_size: self.chunk_size,
            sha1: self.sha1.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ProjectTransferParticipant {
    pub member_id: String,
    pub username: String,
    pub response: String,
    pub progress: f32,
    #[serde(default)]
    pub deadline: Option<u64>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ProjectTransferStatus {
    pub request_id: String,
    pub phase: String,
    pub total_bytes: u64,
    pub transferred_bytes: u64,
    pub participants: Vec<ProjectTransferParticipant>,
    #[serde(default)]
    pub cancel_reason: Option<String>,
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
    ConnectionFailed(String),
    ProtocolMismatch,
    SyncRequested {
        requester: String,
    },
    RoomMetadata {
        member_id: String,
        project_huuid: String,
        project_matches: bool,
        #[allow(dead_code)]
        project_mode: crate::protocol::InvitationProjectMode,
        project_file_name: Option<String>,
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
    AudioUploaded {
        transfer_id: String,
    },
    RecordingTransaction(crate::recording::RecordingTransaction),
    RecordingPrepare(RecordingPreparePayload),
    RecordingCapture(RecordingCapturePayload),
    RecordingPlayback(RecordingPlaybackPayload),
    RecordingView(RecordingViewPayload),
    ActorRequestOpenMicrophone,
    ActorRequestApplyDisplaySettings {
        scroll_speed: f32,
        reading_bar_offset_percent: f32,
    },
    ActorRequestCloseProjectTransferWaiting,
    ProjectTransferAutoRequest {
        member_id: String,
    },
    ProjectTransferRequest(ProjectTransferMetadata),
    ProjectAvailable {
        project_huuid: String,
    },
    ProjectTransferReady(ProjectTransferMetadata),
    ProjectTransferStatus(ProjectTransferStatus),
    ProjectDownloadFinished {
        request_id: String,
        result: Result<PathBuf, String>,
    },
    BigBegin(crate::big_event::BigEventBegin),
    BigChunk {
        transfer_id: String,
        index: usize,
        data_base64: String,
    },
    BigEnd {
        transfer_id: String,
    },
}

/// Outgoing message sent through the single FIFO sender thread.
///
/// Keeping direct and chunked events in the same queue is important: a live
/// recording transaction must not overtake the recording snapshot that brings
/// a newly connected peer to the same transaction-chain tip.
enum OutgoingMessage {
    Direct(String, serde_json::Value),
    Acknowledged(String, serde_json::Value, mpsc::Sender<Result<(), String>>),
    Big(BigSendJob),
}

/// One oversized event queued for the FIFO big sender worker.
struct BigSendJob {
    event: String,
    serialized: Vec<u8>,
    target: Option<String>,
    recording_chain: Option<serde_json::Value>,
}

// The relay does not reassemble large snapshots. Announce the active log head
// so it can resume transaction ordering when the snapshot finishes transferring.
fn recording_snapshot_chain(payload: &serde_json::Value) -> Option<serde_json::Value> {
    let log = payload.get("transactions")?;
    let cursor = usize::try_from(log.get("cursor")?.as_u64()?).ok()?;
    let entries = log.get("entries")?.as_array()?;
    if cursor > entries.len() {
        return None;
    }
    let integrity = if cursor == 0 {
        "0000000000000000"
    } else {
        entries[cursor - 1].get("integrity")?.as_str()?
    };
    Some(serde_json::json!({ "nextSequence": cursor, "previousIntegrity": integrity }))
}

struct AudioSendJob {
    files: Vec<(PathBuf, crate::audio_transfer::AudioTransferMetadata)>,
    result: mpsc::Sender<Result<(), String>>,
}

pub struct NetworkClient {
    _client: Arc<Mutex<Option<rust_socketio::client::Client>>>,
    out_tx: Option<mpsc::SyncSender<OutgoingMessage>>,
    audio_tx: Option<mpsc::Sender<AudioSendJob>>,
    rx: Option<mpsc::Receiver<IncomingMessage>>,
    in_tx: Option<mpsc::Sender<IncomingMessage>>,
    rpc: rpc::RpcClient,
    pub(crate) replication: Option<replication::Replicator>,
    download: Option<(String, Arc<AtomicBool>)>,
    session_id: String,
    /// Packet re-emitted on every automatic reconnect. Starts as the initial
    /// create/join packet and becomes a `join_room` once a room is known, so
    /// the director rejoins its own room instead of creating a new one.
    rejoin_slot: Option<Arc<Mutex<Option<Packet>>>>,
    username: Option<String>,
    local_huuid: Option<String>,
    pub state: ConnectionState,
    pub room_code: Option<String>,
    pub role: Option<String>,
    pub members: Vec<String>,
    pub member_id: Option<String>,
    pub project_huuid: Option<String>,
    pub project_matches: bool,
    /// Policy carried by the invitation used for this connection.
    pub invitation_project_mode: crate::protocol::InvitationProjectMode,
    pub invitation_project_file_name: Option<String>,
    pub sync_requested_this_session: bool,
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
            _client: Arc::new(Mutex::new(None)),
            out_tx: None,
            audio_tx: None,
            rx: None,
            in_tx: None,
            rpc: rpc::RpcClient::new(),
            replication: None,
            download: None,
            session_id: format!("{:032x}", rand::random::<u128>()),
            rejoin_slot: None,
            username: None,
            local_huuid: None,
            state: ConnectionState::Disconnected,
            room_code: None,
            role: None,
            members: Vec::new(),
            member_id: None,
            project_huuid: None,
            project_matches: false,
            invitation_project_mode: crate::protocol::InvitationProjectMode::None,
            invitation_project_file_name: None,
            sync_requested_this_session: false,
            member_details: Vec::new(),
            control_owner_id: None,
        }
    }

    pub fn is_in_room(&self) -> bool {
        self.state == ConnectionState::InRoom
    }

    /// Remember the room to rejoin after an automatic reconnect. Called once
    /// the server confirms room creation or joining.
    pub fn set_rejoin_code(&mut self, code: &str) {
        let (Some(slot), Some(username)) = (&self.rejoin_slot, &self.username) else {
            return;
        };
        let project_huuid = self.rejoin_project_huuid();
        if let Ok(mut slot) = slot.lock() {
            let project_mode = self.invitation_project_mode;
            *slot = Some(Packet::JoinRoom {
                code: code.to_string(),
                username: username.clone(),
                project_huuid,
                project_mode,
            });
        }
    }

    fn rejoin_project_huuid(&self) -> Option<String> {
        if self.project_matches {
            self.project_huuid
                .clone()
                .or_else(|| self.local_huuid.clone())
        } else {
            self.local_huuid.clone()
        }
    }

    /// Local saves get a fresh HUUID. While following the room, keep the
    /// published archive's identity for reconnection so a save does not make
    /// an already-loaded participant wait for a second import.
    pub fn update_local_huuid(&mut self, huuid: Option<String>) {
        if self.rejoin_slot.is_none() {
            return;
        }
        self.local_huuid = huuid;
        let Some(slot) = &self.rejoin_slot else {
            return;
        };
        if let Ok(mut slot) = slot.lock() {
            if let Some(Packet::JoinRoom { project_huuid, .. }) = slot.as_mut() {
                *project_huuid = self.rejoin_project_huuid();
            }
        }
    }

    pub fn is_connected(&self) -> bool {
        matches!(
            self.state,
            ConnectionState::Connected | ConnectionState::InRoom
        )
    }

    pub fn set_project_invitation_mode(
        &self,
        mode: crate::protocol::InvitationProjectMode,
        file_name: Option<&str>,
    ) {
        self.send_raw(
            "set_project_invitation_mode",
            serde_json::json!({
                "project_mode": mode,
                "project_file_name": file_name,
            }),
        );
    }

    pub fn connect_and_send(&mut self, ip: &str, port: u16, password: &str, first_packet: Packet) {
        if self.state != ConnectionState::Disconnected {
            self.disconnect();
        }
        self.state = ConnectionState::Connecting;
        self._client = Arc::new(Mutex::new(None));
        self.rpc = rpc::RpcClient::new();
        self.replication = Some(replication::Replicator::start(self.rpc.clone()));

        match &first_packet {
            Packet::CreateRoom {
                username,
                project_huuid,
            } => {
                self.username = Some(username.clone());
                self.local_huuid = Some(project_huuid.clone());
            }
            Packet::JoinRoom {
                username,
                project_huuid,
                project_mode,
                ..
            } => {
                self.username = Some(username.clone());
                self.local_huuid = project_huuid.clone();
                self.invitation_project_mode = *project_mode;
            }
            _ => {
                self.username = None;
                self.local_huuid = None;
            }
        }
        let rejoin_slot = Arc::new(Mutex::new(None::<Packet>));
        self.rejoin_slot = Some(Arc::clone(&rejoin_slot));

        let (in_tx, in_rx) = mpsc::channel::<IncomingMessage>();
        self.in_tx = Some(in_tx.clone());
        // Bound queued payloads so a multi-gigabyte take cannot be expanded
        // to base64 in memory faster than Socket.IO can emit it.
        let (out_tx, out_rx) = mpsc::sync_channel::<OutgoingMessage>(32);
        let url = format!("http://{}:{}", ip, port);
        log::info!("Connecting to {url}");

        let tx_connect = in_tx.clone();
        let tx_disconnect = in_tx.clone();
        let tx_close = in_tx.clone();
        let tx_room_created = in_tx.clone();
        let tx_version_created = in_tx.clone();
        let tx_version_joined = in_tx.clone();
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
        let tx_audio_uploaded = in_tx.clone();
        let tx_recording_transaction = in_tx.clone();
        let tx_recording_prepare = in_tx.clone();
        let tx_recording_capture = in_tx.clone();
        let tx_recording_playback = in_tx.clone();
        let tx_recording_view = in_tx.clone();
        let tx_actor_request = in_tx.clone();
        let tx_project_transfer_request = in_tx.clone();
        let tx_project_available = in_tx.clone();
        let tx_project_transfer_auto_request = in_tx.clone();
        let tx_project_transfer_ready = in_tx.clone();
        let tx_project_transfer_status = in_tx.clone();
        let tx_big_begin = in_tx.clone();
        let tx_big_chunk = in_tx.clone();
        let tx_big_end = in_tx.clone();

        let rejoin_created = Arc::clone(&rejoin_slot);
        let rejoin_joined = Arc::clone(&rejoin_slot);
        let rejoin_template_created = Packet::JoinRoom {
            code: String::new(),
            username: self.username.clone().unwrap_or_default(),
            project_huuid: self.local_huuid.clone(),
            project_mode: self.invitation_project_mode,
        };
        let rejoin_template_joined = rejoin_template_created.clone();
        let connect_first_packet = first_packet.clone();
        let connect_session_id = self.session_id.clone();
        let out_rx = Mutex::new(Some(out_rx));
        let sender_started = Arc::new(AtomicBool::new(false));
        let sender_started_on_connect = Arc::clone(&sender_started);
        let rpc_reply = self.rpc.clone();
        let rpc_close = self.rpc.clone();
        let rpc_created = self.rpc.clone();
        let rpc_joined = self.rpc.clone();
        let replication_created = self.replication.as_ref().unwrap().signal();
        let replication_joined = replication_created.clone();
        let replication_changed = replication_created.clone();
        let connect_lifetime = self.rpc.clone();
        let sender_lifetime = self.rpc.clone();
        let sender_errors = in_tx.clone();

        let builder = ClientBuilder::new(&url)
            .auth(serde_json::json!({ "password": password, "protocol_version": rpc::PROTOCOL_VERSION }))
            .transport_type(rust_socketio::TransportType::Websocket)
            .reconnect(true)
            .reconnect_delay(500, 10_000)
            .reconnect_on_disconnect(true)
            .on("protocol_reply", move |payload, _| {
                if let Some(value) = payload_to_value(&payload) { rpc_reply.receive(value); }
            })
            .on("state_changed", move |_, _| replication_changed.changed())
            .on(Event::Connect, move |_, client: RawClient| {
                if connect_lifetime.is_stopped() { let _ = client.disconnect(); return; }
                let _ = tx_connect.send(IncomingMessage::Connected);
                // On an automatic reconnect, rejoin the known room instead of
                // re-running the initial packet: re-emitting `create_room`
                // would silently move the director into a fresh, empty room.
                let packet = rejoin_slot
                    .lock()
                    .ok()
                    .and_then(|slot| slot.clone())
                    .unwrap_or_else(|| connect_first_packet.clone());
                let (event, payload) = packet_to_emit(&packet, Some(&connect_session_id));
                let _ = client.emit(event, payload);
                if sender_started_on_connect
                    .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    // Take out_rx once and route it through the socket that most recently
                    // connected. The Socket.IO crate replaces its RawClient on reconnect.
                    if let Some(rx) = out_rx.lock().unwrap().take() {
                        let lifetime = sender_lifetime.clone();
                        let errors = sender_errors.clone();
                        thread::spawn(move || {
                            run_outgoing_sender(rx, lifetime, errors);
                        });
                    }
                }
            })
            .on(Event::Close, move |_, _| {
                rpc_close.set_client(None);
                // Notify the app that the transport dropped. Without this the
                // session state (room, sync request flag) was never reset on
                // an automatic reconnect, leaving peers stuck waiting for a
                // sync they no longer requested. On a manual disconnect the
                // receiver is dropped right after `client.disconnect()`, so
                // this message is discarded.
                let _ = tx_close.send(IncomingMessage::Disconnected("transport closed".into()));
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
            .on("room_created", move |payload, client| {
                if payload_to_value(&payload).and_then(|value| value["protocol_version"].as_u64()) != Some(rpc::PROTOCOL_VERSION as u64) {
                    let _ = tx_version_created.send(IncomingMessage::ProtocolMismatch);
                    return;
                }
                rpc_created.set_client(Some(client));
                replication_created.admitted(true);
                let code = extract_string_field(&payload, "code");
                let mut rejoin = rejoin_template_created.clone();
                if let Packet::JoinRoom { code: target, .. } = &mut rejoin { *target = code.clone(); }
                *rejoin_created.lock().unwrap() = Some(rejoin);
                let member_id = extract_string_field(&payload, "member_id");
                let project_huuid = extract_string_field(&payload, "project_huuid");
                let _ = tx_room_metadata_created.send(IncomingMessage::RoomMetadata {
                    member_id,
                    project_huuid,
                    project_matches: true,
                    project_mode: crate::protocol::InvitationProjectMode::None,
                    project_file_name: None,
                });
                let _ = tx_room_created.send(IncomingMessage::Packet(Packet::RoomCreated { code }));
            })
            .on("room_joined", move |payload, client: RawClient| {
                if payload_to_value(&payload).and_then(|value| value["protocol_version"].as_u64()) != Some(rpc::PROTOCOL_VERSION as u64) {
                    let _ = tx_version_joined.send(IncomingMessage::ProtocolMismatch);
                    return;
                }
                rpc_joined.set_client(Some(client));
                if let Some(obj) = payload_to_value(&payload) {
                    let code = obj["code"].as_str().unwrap_or("").to_string();
                    let mut slot = rejoin_joined.lock().unwrap();
                    let mut rejoin = slot.clone().unwrap_or_else(|| rejoin_template_joined.clone());
                    if let Packet::JoinRoom { code: target, .. } = &mut rejoin { *target = code.clone(); }
                    *slot = Some(rejoin);
                    drop(slot);
                    let role = obj["role"].as_str().unwrap_or("user").to_string();
                    replication_joined.admitted(role == "admin");
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
                    let project_matches = obj["project_matches"].as_bool().unwrap_or(false);
                    let project_mode =
                        serde_json::from_value(obj["project_mode"].clone()).unwrap_or_default();
                    let project_file_name =
                        obj["project_file_name"].as_str().map(ToOwned::to_owned);
                    let _ = tx_room_metadata_joined.send(IncomingMessage::RoomMetadata {
                        member_id,
                        project_huuid,
                        project_matches,
                        project_mode,
                        project_file_name,
                    });
                    let _ = tx_room_joined.send(IncomingMessage::Packet(Packet::RoomJoined {
                        code,
                        role,
                        members,
                    }));
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
            .on("audio_uploaded", move |payload, _| {
                if let Some(obj) = payload_to_value(&payload) {
                    let transfer_id = obj["transfer_id"].as_str().unwrap_or("").to_string();
                    let _ = tx_audio_uploaded.send(IncomingMessage::AudioUploaded { transfer_id });
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
            })
            .on("recording_view", move |payload, _| {
                if let Some(value) = payload_to_value(&payload) {
                    match serde_json::from_value(value) {
                        Ok(view) => {
                            let _ = tx_recording_view.send(IncomingMessage::RecordingView(view));
                        }
                        Err(error) => {
                            let _ = tx_recording_view.send(IncomingMessage::Error(format!(
                                "invalid recording view: {error}"
                            )));
                        }
                    }
                }
            })
            .on("actor_request", move |payload, _| {
                let Some(value) = payload_to_value(&payload) else {
                    return;
                };
                match value["action"].as_str() {
                    Some("open_microphone") => {
                        let _ = tx_actor_request.send(IncomingMessage::ActorRequestOpenMicrophone);
                    }
                    Some("apply_display_settings") => {
                        let (Some(scroll_speed), Some(reading_bar_offset_percent)) = (
                            value["scroll_speed"].as_f64(),
                            value["reading_bar_offset_percent"].as_f64(),
                        ) else {
                            return;
                        };
                        let _ = tx_actor_request.send(
                            IncomingMessage::ActorRequestApplyDisplaySettings {
                                scroll_speed: scroll_speed as f32,
                                reading_bar_offset_percent: reading_bar_offset_percent as f32,
                            },
                        );
                    }
                    Some("close_project_transfer_waiting") => {
                        let _ = tx_actor_request
                            .send(IncomingMessage::ActorRequestCloseProjectTransferWaiting);
                    }
                    _ => {}
                }
            })
            .on("project_transfer_request", move |payload, _| {
                if let Some(value) = payload_to_value(&payload) {
                    if let Ok(metadata) = serde_json::from_value(value) {
                        let _ = tx_project_transfer_request
                            .send(IncomingMessage::ProjectTransferRequest(metadata));
                    }
                }
            })
            .on("project_available", move |payload, _| {
                let project_huuid = extract_string_field(&payload, "project_huuid");
                let _ = tx_project_available.send(IncomingMessage::ProjectAvailable { project_huuid });
            })
            .on("project_transfer_auto_request", move |payload, _| {
                if let Some(value) = payload_to_value(&payload) {
                    let member_id = value["member_id"].as_str().unwrap_or("").to_string();
                    if !member_id.is_empty() {
                        let _ = tx_project_transfer_auto_request
                            .send(IncomingMessage::ProjectTransferAutoRequest { member_id });
                    }
                }
            })
            .on("project_transfer_ready", move |payload, _| {
                if let Some(value) = payload_to_value(&payload) {
                    if let Ok(metadata) = serde_json::from_value(value["metadata"].clone()) {
                        let _ = tx_project_transfer_ready
                            .send(IncomingMessage::ProjectTransferReady(metadata));
                    }
                }
            })
            .on("project_transfer_status", move |payload, _| {
                if let Some(value) = payload_to_value(&payload) {
                    if let Ok(status) = serde_json::from_value(value) {
                        let _ = tx_project_transfer_status
                            .send(IncomingMessage::ProjectTransferStatus(status));
                    }
                }
            })
            .on("big_begin", move |payload, _| {
                let Some(value) = payload_to_value(&payload) else {
                    return;
                };
                let begin = crate::big_event::BigEventBegin {
                    transfer_id: value["transfer_id"].as_str().unwrap_or("").to_string(),
                    event: value["event"].as_str().unwrap_or("").to_string(),
                    total_bytes: value["total_bytes"].as_u64().unwrap_or(0),
                    total_chunks: value["total_chunks"].as_u64().unwrap_or(0) as usize,
                    chunk_size: value["chunk_size"].as_u64().unwrap_or(0) as usize,
                    sha1: value["sha1"].as_str().unwrap_or("").to_string(),
                };
                let _ = tx_big_begin.send(IncomingMessage::BigBegin(begin));
            })
            .on("big_chunk", move |payload, _| {
                if let Some(value) = payload_to_value(&payload) {
                    let transfer_id = value["transfer_id"].as_str().unwrap_or("").to_string();
                    let index = value["index"].as_u64().unwrap_or(0) as usize;
                    let data_base64 = value["data"].as_str().unwrap_or("").to_string();
                    let _ = tx_big_chunk.send(IncomingMessage::BigChunk {
                        transfer_id,
                        index,
                        data_base64,
                    });
                }
            })
            .on("big_end", move |payload, _| {
                let transfer_id = payload_to_value(&payload)
                    .and_then(|value| value["transfer_id"].as_str().map(String::from))
                    .unwrap_or_default();
                let _ = tx_big_end.send(IncomingMessage::BigEnd { transfer_id });
            });

        let (audio_tx, audio_rx) = mpsc::channel();
        let audio_out_tx = out_tx.clone();
        thread::spawn(move || run_audio_sender(audio_rx, audio_out_tx));
        self.audio_tx = Some(audio_tx);
        self.out_tx = Some(out_tx);
        self.rx = Some(in_rx);

        let client_slot = self._client.clone();
        let lifetime = self.rpc.clone();
        // DNS, TCP and the WebSocket handshake may all stall. None of them
        // belongs on the UI thread, including an unsuccessful first attempt.
        thread::spawn(move || {
            let result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| builder.connect()));
            let result = match result {
                Ok(Ok(client)) => Ok(client),
                Ok(Err(error)) => Err(error.to_string()),
                Err(_) => Err("Connexion échouée (panic)".into()),
            };
            match result {
                Ok(client) => {
                    let mut slot = client_slot.lock().unwrap();
                    if lifetime.is_stopped() {
                        drop(slot);
                        let _ = client.disconnect();
                    } else {
                        *slot = Some(client);
                    }
                }
                Err(error) if !lifetime.is_stopped() => {
                    lifetime.stop();
                    log::error!("Socket.io connection failed: {error}");
                    let _ = in_tx.send(IncomingMessage::ConnectionFailed(error));
                }
                Err(_) => {}
            }
        });
    }

    /// Send a packet via the sender thread.
    pub fn send(&self, packet: &Packet) {
        let (event, payload) = packet_to_emit(packet, None);
        self.send_raw(event, payload);
    }

    /// Send a raw event via the sender thread.
    pub fn send_raw(&self, event: &str, payload: serde_json::Value) {
        log::debug!("Sending event: {event}");
        self.enqueue(OutgoingMessage::Direct(event.to_string(), payload));
    }

    fn enqueue(&self, message: OutgoingMessage) {
        if let Some(tx) = &self.out_tx {
            if let Err(error) = tx.try_send(message) {
                log::error!("outgoing network queue unavailable: {error}");
                if let Some(tx) = &self.in_tx {
                    let _ = tx.send(IncomingMessage::Error(
                        "La file réseau est saturée. L’action n’a pas été envoyée.".into(),
                    ));
                }
            }
        }
    }

    pub fn send_recording_transaction(&self, transaction: &crate::recording::RecordingTransaction) {
        if let Ok(payload) = serde_json::to_value(transaction) {
            self.send_raw("recording_transaction", payload);
        }
    }

    pub fn initialize_created_room(&mut self, code: &str, prepare: &RecordingPreparePayload) {
        self.state = ConnectionState::InRoom;
        self.room_code = Some(code.to_owned());
        self.role = Some("admin".into());
        self.set_rejoin_code(code);
        // Even a project with no takes already has the default-track transaction.
        // Initialize the server chain before any subsequent mute/solo operation.
        self.send_recording_prepare(prepare);
    }

    /// Send an event directly when its serialized payload fits in one
    /// websocket frame, otherwise hand it to the FIFO big sender worker which
    /// frames it as `big_begin` / `big_chunk`* / `big_end`.
    pub fn send_big_event(
        &self,
        event: &str,
        mut payload: serde_json::Value,
        target: Option<&str>,
    ) {
        let serialized = match serde_json::to_vec(&payload) {
            Ok(serialized) => serialized,
            Err(error) => {
                log::error!("cannot serialize {event}: {error}");
                return;
            }
        };
        if serialized.len() <= crate::big_event::BIG_EVENT_DIRECT_MAX_BYTES {
            if let Some(target) = target {
                payload["_target"] = serde_json::Value::String(target.to_owned());
            }
            self.enqueue(OutgoingMessage::Direct(event.to_string(), payload));
            return;
        }
        if serialized.len() as u64 > crate::big_event::MAX_BIG_EVENT_BYTES {
            log::error!("{event} payload exceeds the big event size limit");
            return;
        }
        let Some(_) = &self.out_tx else {
            log::warn!("dropping oversized {event}: network is not connected");
            return;
        };
        let job = BigSendJob {
            event: event.to_string(),
            serialized,
            target: target.map(str::to_owned),
            recording_chain: (event == "recording_prepare")
                .then(|| recording_snapshot_chain(&payload))
                .flatten(),
        };
        self.enqueue(OutgoingMessage::Big(job));
    }

    pub fn send_recording_prepare(&self, prepare: &RecordingPreparePayload) {
        if let Ok(payload) = serde_json::to_value(prepare) {
            self.send_big_event("recording_prepare", payload, None);
        }
    }

    pub fn send_recording_prepare_to(&self, prepare: &RecordingPreparePayload, member_id: &str) {
        if let Ok(payload) = serde_json::to_value(prepare) {
            self.send_big_event("recording_prepare", payload, Some(member_id));
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

    pub fn send_recording_ready(&self, ready: bool) {
        self.send_raw("recording_ready", serde_json::json!({ "ready": ready }));
    }

    pub fn send_recording_playback(&self, frame: i64, playing: bool) {
        let payload = RecordingPlaybackPayload { frame, playing };
        if let Ok(payload) = serde_json::to_value(payload) {
            self.send_raw("recording_playback", payload);
        }
    }

    pub fn send_recording_view(&self, view: RecordingViewPayload, target: Option<&str>) {
        if let Ok(mut payload) = serde_json::to_value(view) {
            if let Some(target) = target {
                payload["_target"] = serde_json::Value::String(target.to_owned());
            }
            self.send_raw("recording_view", payload);
        }
    }

    pub fn request_project_transfer(&self, metadata: &ProjectTransferMetadata) {
        self.request_project_transfer_to(metadata, None);
    }

    pub fn request_project_transfer_to(
        &self,
        metadata: &ProjectTransferMetadata,
        member_id: Option<&str>,
    ) {
        let rpc = self.rpc.clone();
        let Some(in_tx) = self.in_tx.clone() else {
            return;
        };
        let metadata = metadata.clone();
        let mut payload = serde_json::to_value(&metadata).expect("transfer metadata serializes");
        if let Some(id) = member_id {
            payload["member_id"] = id.into();
        }
        thread::spawn(move || match rpc.call("project_begin", payload) {
            Ok(reply) => match serde_json::from_value(reply["metadata"].clone()) {
                Ok(canonical) => {
                    let _ = in_tx.send(IncomingMessage::ProjectTransferReady(canonical));
                }
                Err(error) => {
                    let _ = in_tx.send(IncomingMessage::Error(format!(
                        "invalid upload metadata: {error}"
                    )));
                }
            },
            Err(error) => {
                let _ = in_tx.send(IncomingMessage::ProjectTransferStatus(
                    ProjectTransferStatus {
                        request_id: metadata.request_id,
                        phase: "cancelled".into(),
                        total_bytes: metadata.total_bytes,
                        transferred_bytes: 0,
                        participants: Vec::new(),
                        cancel_reason: Some(error.clone()),
                    },
                ));
                let _ = in_tx.send(IncomingMessage::Error(error));
            }
        });
    }

    fn project_rpc(&self, method: &'static str, payload: serde_json::Value) {
        let rpc = self.rpc.clone();
        let in_tx = self.in_tx.clone();
        thread::spawn(move || {
            if let Err(error) = rpc.call(method, payload) {
                if let Some(tx) = in_tx {
                    let _ = tx.send(IncomingMessage::Error(error));
                }
            }
        });
    }

    pub fn respond_project_transfer(&self, request_id: &str, response: &str) {
        // Acceptance is acknowledged by the download worker before reading.
        if response != "accepted" {
            self.project_rpc(
                "project_response",
                serde_json::json!({ "request_id": request_id, "response": response }),
            );
        }
    }

    pub fn report_project_transfer_loading(&self, request_id: &str) {
        self.project_rpc(
            "project_loading",
            serde_json::json!({ "request_id": request_id }),
        );
    }

    pub fn report_project_transfer(&self, request_id: &str, success: bool, error: Option<&str>) {
        self.project_rpc(
            "project_result",
            serde_json::json!({ "request_id": request_id, "success": success, "error": error }),
        );
    }

    pub fn abort_project_upload(&self, request_id: &str, error: &str) {
        self.project_rpc(
            "project_abort",
            serde_json::json!({ "request_id": request_id, "error": error }),
        );
    }

    pub fn send_project_file(
        &self,
        path: PathBuf,
        metadata: ProjectTransferMetadata,
    ) -> mpsc::Receiver<Result<(), String>> {
        let (result_tx, result_rx) = mpsc::channel();
        let rpc = self.rpc.clone();
        thread::spawn(move || {
            let result = (|| {
                let generic = metadata.file_metadata();
                generic.validate()?;
                let begin = rpc.call(
                    "project_begin",
                    serde_json::to_value(&metadata).map_err(|error| error.to_string())?,
                )?;
                if begin["complete"] == true {
                    return Ok(());
                }
                let next = begin["next_index"]
                    .as_u64()
                    .ok_or("invalid upload acknowledgement")? as usize;
                for chunk in crate::file_transfer::FileChunkReader::open_at(&path, &generic, next)?
                {
                    let (index, data) = chunk?;
                    let reply = rpc.call("project_write", serde_json::json!({ "request_id": metadata.request_id, "index": index, "data": data }))?;
                    if reply["next_index"].as_u64() != Some(index as u64 + 1) {
                        return Err("invalid chunk acknowledgement".into());
                    }
                }
                rpc.call(
                    "project_commit",
                    serde_json::json!({ "request_id": metadata.request_id }),
                )?;
                Ok(())
            })();
            let _ = result_tx.send(result);
        });
        result_rx
    }

    pub fn cancel_project_download(&mut self) {
        if let Some((_, cancelled)) = self.download.take() {
            cancelled.store(true, Ordering::Release);
        }
    }

    pub fn download_project(&mut self, metadata: ProjectTransferMetadata, destination: PathBuf) {
        if self
            .download
            .as_ref()
            .is_some_and(|(id, _)| id == &metadata.request_id)
        {
            return;
        }
        self.cancel_project_download();
        let Some(in_tx) = self.in_tx.clone() else {
            return;
        };
        let rpc = self.rpc.clone();
        let cancelled = Arc::new(AtomicBool::new(false));
        self.download = Some((metadata.request_id.clone(), cancelled.clone()));
        thread::spawn(move || {
            let result = (|| {
                let mut receiver = crate::file_transfer::FileTransferReceiver::default();
                receiver.begin(metadata.file_metadata(), &destination)?;
                rpc.call_cancellable("project_response", serde_json::json!({ "request_id": metadata.request_id, "response": "accepted" }), &cancelled)?;
                for index in 0..metadata.total_chunks {
                    let reply = rpc.call_cancellable(
                        "project_read",
                        serde_json::json!({ "request_id": metadata.request_id, "index": index }),
                        &cancelled,
                    )?;
                    if reply["request_id"].as_str() != Some(&metadata.request_id)
                        || reply["index"].as_u64() != Some(index as u64)
                    {
                        return Err("invalid project download reply".into());
                    }
                    receiver.push_base64(
                        index,
                        reply["data"].as_str().ok_or("missing project chunk")?,
                    )?;
                }
                let received = receiver.finish(&metadata.request_id)?;
                rpc.call_cancellable(
                    "project_loading",
                    serde_json::json!({ "request_id": metadata.request_id }),
                    &cancelled,
                )?;
                Ok(received.path)
            })();
            if !cancelled.load(Ordering::Acquire) {
                let _ = in_tx.send(IncomingMessage::ProjectDownloadFinished {
                    request_id: metadata.request_id,
                    result,
                });
            }
        });
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
        self.send_audio_files(vec![(path, metadata)])
    }

    /// Stream several FLAC files without interleaving their start/chunk/end
    /// frames. The server intentionally permits one active audio transfer per
    /// sender, so catch-up publication must remain strictly sequential.
    pub fn send_audio_files(
        &self,
        files: Vec<(PathBuf, crate::audio_transfer::AudioTransferMetadata)>,
    ) -> mpsc::Receiver<Result<(), String>> {
        let (result_tx, result_rx) = mpsc::channel();
        let Some(audio_tx) = &self.audio_tx else {
            let _ = result_tx.send(Err("network is not connected".into()));
            return result_rx;
        };
        if let Err(error) = audio_tx.send(AudioSendJob {
            files,
            result: result_tx,
        }) {
            let _ = error
                .0
                .result
                .send(Err("recording audio sender stopped".into()));
        }
        result_rx
    }

    pub fn try_recv(&self) -> Option<IncomingMessage> {
        self.rx.as_ref()?.try_recv().ok()
    }

    pub fn disconnect(&mut self) {
        log::info!("Disconnecting from server");
        self.rpc.stop();
        self.replication = None;
        self.cancel_project_download();
        self.in_tx = None;
        // Drop out_tx first to stop sender thread
        self.audio_tx = None;
        self.out_tx = None;
        if let Some(client) = self._client.lock().unwrap().take() {
            thread::spawn(move || {
                let _ = client.disconnect();
            });
        }
        self.rx = None;
        self.rejoin_slot = None;
        self.username = None;
        self.local_huuid = None;
        self.state = ConnectionState::Disconnected;
        self.room_code = None;
        self.role = None;
        self.members.clear();
        self.member_id = None;
        self.project_huuid = None;
        self.project_matches = false;
        self.sync_requested_this_session = false;
        self.invitation_project_mode = crate::protocol::InvitationProjectMode::None;
        self.invitation_project_file_name = None;
        self.member_details.clear();
        self.control_owner_id = None;
    }
}

impl Drop for NetworkClient {
    fn drop(&mut self) {
        self.disconnect();
    }
}

fn packet_to_emit(packet: &Packet, session_id: Option<&str>) -> (&'static str, serde_json::Value) {
    match packet {
        Packet::CreateRoom {
            username,
            project_huuid,
        } => {
            let mut payload = serde_json::json!({
                "username": username,
                "project_huuid": project_huuid,
            });
            if let Some(session_id) = session_id {
                payload["session_id"] = serde_json::Value::String(session_id.to_owned());
            }
            ("create_room", payload)
        }
        Packet::JoinRoom {
            code,
            username,
            project_huuid,
            project_mode,
        } => {
            let mut payload = serde_json::json!({
                "code": code,
                "username": username,
                "project_huuid": project_huuid,
                "project_mode": project_mode,
            });
            if let Some(session_id) = session_id {
                payload["session_id"] = serde_json::Value::String(session_id.to_owned());
            }
            ("join_room", payload)
        }
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

fn run_audio_sender(rx: mpsc::Receiver<AudioSendJob>, out_tx: mpsc::SyncSender<OutgoingMessage>) {
    // Keep at most one audio block in the FIFO so interactive room commands
    // can run between blocks. Success means that the server accepted audio_end.
    let send = |event: &str, payload| -> Result<(), String> {
        let (result, received) = mpsc::channel();
        out_tx
            .send(OutgoingMessage::Acknowledged(event.into(), payload, result))
            .map_err(|_| "network sender stopped".to_string())?;
        received
            .recv()
            .map_err(|_| "network sender stopped".to_string())?
    };
    while let Ok(job) = rx.recv() {
        let result = (|| {
            for (path, metadata) in job.files {
                metadata.validate()?;
                send(
                    "audio_start",
                    serde_json::to_value(&metadata).map_err(|error| error.to_string())?,
                )?;
                for chunk in crate::audio_transfer::AudioChunkReader::open(&path, &metadata)? {
                    let chunk = chunk?;
                    send(
                        "audio_chunk",
                        serde_json::json!({"transfer_id": metadata.transfer_id,
                        "index": chunk.index, "data": chunk.data_base64}),
                    )?;
                }
                send(
                    "audio_end",
                    serde_json::json!({"transfer_id": metadata.transfer_id}),
                )?;
            }
            Ok(())
        })();
        let sender_stopped = matches!(&result, Err(error) if error == "network sender stopped");
        let _ = job.result.send(result);
        if sender_stopped {
            return;
        }
    }
}

/// Sender worker loop. Direct and chunked events are consumed from one FIFO so
/// a live transaction cannot overtake a preceding snapshot or sync event.
fn run_outgoing_sender(
    rx: mpsc::Receiver<OutgoingMessage>,
    lifetime: rpc::RpcClient,
    errors: mpsc::Sender<IncomingMessage>,
) {
    use std::sync::atomic::AtomicU64;
    use std::time::{SystemTime, UNIX_EPOCH};

    static BIG_TRANSFER_COUNTER: AtomicU64 = AtomicU64::new(0);

    while !lifetime.is_stopped() {
        let message = match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(message) => message,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        match message {
            OutgoingMessage::Direct(event, payload) => {
                if let Err(error) = emit_with_retry(&lifetime, &event, &payload) {
                    let _ = errors.send(IncomingMessage::Error(error));
                }
            }
            OutgoingMessage::Acknowledged(event, payload, result) => {
                let _ = result.send(emit_with_retry(&lifetime, &event, &payload));
            }
            OutgoingMessage::Big(job) => {
                let result: Result<(), String> = (|| {
                    let (frame, chunks) = crate::big_event::frame_big_event(&job.serialized)?;
                    let nanos = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|duration| duration.as_nanos())
                        .unwrap_or(0);
                    let transfer_id = format!(
                        "big_{nanos}_{}",
                        BIG_TRANSFER_COUNTER.fetch_add(1, Ordering::Relaxed)
                    );
                    let mut begin = serde_json::json!({
                        "transfer_id": transfer_id,
                        "event": job.event,
                        "total_bytes": frame.total_bytes,
                        "total_chunks": frame.total_chunks,
                        "chunk_size": frame.chunk_size,
                        "sha1": frame.sha1,
                    });
                    if let Some(target) = &job.target {
                        begin["_target"] = serde_json::Value::String(target.clone());
                    }
                    if let Some(chain) = &job.recording_chain {
                        begin["recording_chain"] = chain.clone();
                    }
                    emit_with_retry(&lifetime, "big_begin", &begin)?;
                    for (index, data) in chunks.iter().enumerate() {
                        let payload = serde_json::json!({
                            "transfer_id": transfer_id,
                            "index": index,
                            "data": data,
                        });
                        emit_with_retry(&lifetime, "big_chunk", &payload)?;
                    }
                    let payload = serde_json::json!({ "transfer_id": transfer_id });
                    emit_with_retry(&lifetime, "big_end", &payload)?;
                    log::info!(
                        "Sent chunked {} ({} bytes, {} chunks)",
                        job.event,
                        frame.total_bytes,
                        frame.total_chunks
                    );
                    Ok(())
                })();
                if let Err(error) = result {
                    log::error!("cannot send chunked {}: {error}", job.event);
                    let _ = errors.send(IncomingMessage::Error(error));
                }
            }
        }
    }
}

fn emit_with_retry(
    lifetime: &rpc::RpcClient,
    event: &str,
    payload: &serde_json::Value,
) -> Result<(), String> {
    lifetime
        .call(
            "event",
            serde_json::json!({ "event": event, "payload": payload }),
        )
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn a_silent_server_does_not_block_connect_or_cancellation() {
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(2);
            while std::time::Instant::now() < deadline {
                if let Ok((connection, _)) = listener.accept() {
                    thread::sleep(Duration::from_secs(1));
                    drop(connection);
                    break;
                }
                thread::sleep(Duration::from_millis(10));
            }
        });
        let mut network = NetworkClient::new();
        let started = std::time::Instant::now();
        network.connect_and_send(
            "127.0.0.1",
            port,
            "",
            Packet::CreateRoom {
                username: "DA".into(),
                project_huuid: "project".into(),
            },
        );
        assert!(
            started.elapsed() < Duration::from_millis(500),
            "connection blocked its caller"
        );
        let cancelled = std::time::Instant::now();
        network.disconnect();
        assert!(
            cancelled.elapsed() < Duration::from_millis(500),
            "cancellation blocked its caller"
        );
        assert!(network.rpc.is_stopped());
        server.join().unwrap();
    }

    #[test]
    fn a_local_save_preserves_the_published_archive_identity_for_reconnection() {
        let mut network = NetworkClient::new();
        network.username = Some("Actor".into());
        network.project_matches = true;
        network.project_huuid = Some("published-archive".into());
        network.local_huuid = Some("published-archive".into());
        network.rejoin_slot = Some(Arc::new(Mutex::new(None)));
        network.set_rejoin_code("ABC123");
        network.update_local_huuid(Some("new-local-save".into()));
        network.set_rejoin_code("ABC123");
        let slot = network.rejoin_slot.as_ref().unwrap().lock().unwrap();
        let Some(Packet::JoinRoom { project_huuid, .. }) = slot.as_ref() else {
            panic!("missing rejoin packet")
        };
        assert_eq!(project_huuid.as_deref(), Some("published-archive"));
        assert_eq!(network.local_huuid.as_deref(), Some("new-local-save"));
    }

    #[test]
    fn created_room_prepares_the_default_track_before_mute_and_solo() {
        use crate::recording::{RecordingOperation, RecordingProject, TransactionLog};

        let (sender, receiver) = mpsc::sync_channel(4);
        let mut network = NetworkClient::new();
        network.out_tx = Some(sender);
        let session = crate::application::project_service::ProjectSession::new();
        let mut project = session.recording_project;
        let mut transactions = session.recording_transactions;
        assert_eq!(project.clips().len(), 0);
        let track_id = project.tracks().next().unwrap().id;

        network.initialize_created_room(
            "ABCDEF",
            &RecordingPreparePayload {
                project: project.clone(),
                transactions: transactions.clone(),
                current_frame: 0,
                capture_target: None,
            },
        );
        let OutgoingMessage::Direct(event, payload) = receiver
            .try_recv()
            .expect("room creation must initialize the server recording journal")
        else {
            panic!("expected initial recording snapshot");
        };
        assert_eq!(event, "recording_prepare");
        let prepare: RecordingPreparePayload = serde_json::from_value(payload).unwrap();
        let mut remote = RecordingProject::new(project.timeline_fps()).unwrap();
        let mut remote_log: TransactionLog = prepare.transactions;
        remote = remote_log.rebuild_from_base(&remote).unwrap();
        assert_eq!(remote, prepare.project);

        for operation in [
            RecordingOperation::SetTrackMuted {
                track_id,
                muted: true,
            },
            RecordingOperation::SetTrackSolo {
                track_id,
                solo: true,
            },
            RecordingOperation::SetTrackMuted {
                track_id,
                muted: false,
            },
        ] {
            let transaction = transactions
                .append_and_apply(&mut project, operation)
                .unwrap();
            network.send_recording_transaction(transaction);
            let OutgoingMessage::Direct(event, payload) = receiver.recv().unwrap() else {
                panic!("expected recording transaction");
            };
            assert_eq!(event, "recording_transaction");
            remote_log
                .append_received_and_apply(&mut remote, serde_json::from_value(payload).unwrap())
                .unwrap();
            assert_eq!(remote, project);
            assert_eq!(
                remote.is_track_audible(track_id),
                project.is_track_audible(track_id)
            );
        }
    }

    #[test]
    fn recording_view_preserves_absolute_instrumental_offset() {
        for offset in [-48, 0, 72] {
            let wire = serde_json::json!({
                "language_id": 1, "instrumental": true,
                "instrumental_audio_offset_frames": offset
            });
            let view: RecordingViewPayload = serde_json::from_value(wire.clone()).unwrap();
            assert_eq!(view.instrumental_audio_offset_frames, Some(offset));
            assert_eq!(serde_json::to_value(view).unwrap(), wire);
        }
        let legacy: RecordingViewPayload = serde_json::from_value(serde_json::json!({
            "language_id": 1, "instrumental": false
        }))
        .unwrap();
        assert_eq!(legacy.instrumental_audio_offset_frames, None);
    }

    #[test]
    fn chunked_recording_snapshot_announces_active_cursor_including_undo() {
        for cursor in [0, 1, 2] {
            let (sender, receiver) = mpsc::sync_channel(1);
            let mut network = NetworkClient::new();
            network.out_tx = Some(sender);
            network.send_big_event(
                "recording_prepare",
                serde_json::json!({
                    "project": "x".repeat(crate::big_event::BIG_EVENT_DIRECT_MAX_BYTES),
                    "transactions": { "cursor": cursor, "entries": [
                        { "integrity": "aaaaaaaaaaaaaaaa" },
                        { "integrity": "bbbbbbbbbbbbbbbb" }
                    ] }
                }),
                Some("actor"),
            );
            let OutgoingMessage::Big(job) = receiver.recv().unwrap() else {
                panic!("expected a chunked snapshot");
            };
            assert_eq!(
                job.recording_chain,
                Some(serde_json::json!({
                    "nextSequence": cursor,
                    "previousIntegrity": (["0000000000000000", "aaaaaaaaaaaaaaaa", "bbbbbbbbbbbbbbbb"][cursor])
                }))
            );
        }
    }

    #[test]
    fn chunked_snapshot_stays_before_following_live_transaction() {
        let (sender, receiver) = mpsc::sync_channel(4);
        let mut network = NetworkClient::new();
        network.out_tx = Some(sender);

        network.send_big_event(
            "recording_prepare",
            serde_json::json!({ "snapshot": "x".repeat(crate::big_event::BIG_EVENT_DIRECT_MAX_BYTES + 1) }),
            None,
        );
        network.send_raw(
            "recording_transaction",
            serde_json::json!({ "operation": { "op": "set_track_muted" } }),
        );

        match receiver.recv().unwrap() {
            OutgoingMessage::Big(job) => assert_eq!(job.event, "recording_prepare"),
            OutgoingMessage::Direct(event, _) => panic!("received direct event first: {event}"),
            OutgoingMessage::Acknowledged(..) => panic!("unexpected audio event"),
        }
        match receiver.recv().unwrap() {
            OutgoingMessage::Direct(event, _) => assert_eq!(event, "recording_transaction"),
            OutgoingMessage::Big(job) => panic!("received chunked event second: {}", job.event),
            OutgoingMessage::Acknowledged(..) => panic!("unexpected audio event"),
        }
    }
}
