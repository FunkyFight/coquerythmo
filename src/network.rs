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
    SyncRequested {
        requester: String,
    },
    RoomMetadata {
        member_id: String,
        project_huuid: String,
        project_matches: bool,
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
    ProjectTransferRequest(ProjectTransferMetadata),
    ProjectTransferReady(ProjectTransferMetadata),
    ProjectTransferStatus(ProjectTransferStatus),
    ProjectTransferChunk {
        request_id: String,
        index: usize,
        data_base64: String,
    },
    ProjectTransferEnd {
        request_id: String,
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
    Big(BigSendJob),
}

/// One oversized event queued for the FIFO big sender worker.
struct BigSendJob {
    event: String,
    serialized: Vec<u8>,
    target: Option<String>,
}

struct AudioSendJob {
    files: Vec<(PathBuf, crate::audio_transfer::AudioTransferMetadata)>,
    result: mpsc::Sender<Result<(), String>>,
}

pub struct NetworkClient {
    _client: Option<rust_socketio::client::Client>,
    out_tx: Option<mpsc::SyncSender<OutgoingMessage>>,
    audio_tx: Option<mpsc::Sender<AudioSendJob>>,
    rx: Option<mpsc::Receiver<IncomingMessage>>,
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
            _client: None,
            out_tx: None,
            audio_tx: None,
            rx: None,
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
        let project_huuid = self.local_huuid.clone();
        if let Ok(mut slot) = slot.lock() {
            *slot = Some(Packet::JoinRoom {
                code: code.to_string(),
                username: username.clone(),
                project_huuid,
            });
        }
    }

    /// Keep the rejoin packet's project HUUID in sync with the local project.
    /// Every successful save or import assigns a fresh HUUID; rejoining with
    /// a stale one would report `project_matches = false` and skip the sync
    /// request that restores the session.
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
                *project_huuid = self.local_huuid.clone();
            }
        }
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
                ..
            } => {
                self.username = Some(username.clone());
                self.local_huuid = project_huuid.clone();
            }
            _ => {
                self.username = None;
                self.local_huuid = None;
            }
        }
        let rejoin_slot = Arc::new(Mutex::new(None::<Packet>));
        self.rejoin_slot = Some(Arc::clone(&rejoin_slot));

        let (in_tx, in_rx) = mpsc::channel::<IncomingMessage>();
        // Bound queued payloads so a multi-gigabyte take cannot be expanded
        // to base64 in memory faster than Socket.IO can emit it.
        let (out_tx, out_rx) = mpsc::sync_channel::<OutgoingMessage>(32);
        let url = format!("http://{}:{}", ip, port);
        log::info!("Connecting to {url}");

        let tx_connect = in_tx.clone();
        let tx_disconnect = in_tx.clone();
        let tx_close = in_tx.clone();
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
        let tx_audio_uploaded = in_tx.clone();
        let tx_recording_transaction = in_tx.clone();
        let tx_recording_prepare = in_tx.clone();
        let tx_recording_capture = in_tx.clone();
        let tx_recording_playback = in_tx.clone();
        let tx_recording_view = in_tx.clone();
        let tx_actor_request = in_tx.clone();
        let tx_project_transfer_request = in_tx.clone();
        let tx_project_transfer_ready = in_tx.clone();
        let tx_project_transfer_status = in_tx.clone();
        let tx_project_transfer_chunk = in_tx.clone();
        let tx_project_transfer_end = in_tx.clone();
        let tx_big_begin = in_tx.clone();
        let tx_big_chunk = in_tx.clone();
        let tx_big_end = in_tx.clone();

        let connect_first_packet = first_packet.clone();
        let connect_session_id = self.session_id.clone();
        let out_rx = Mutex::new(Some(out_rx));
        let sender_client = Arc::new(Mutex::new(None::<RawClient>));
        let sender_started = Arc::new(AtomicBool::new(false));
        let sender_client_on_connect = Arc::clone(&sender_client);
        let sender_client_on_close = Arc::clone(&sender_client);
        let sender_client_for_thread = Arc::clone(&sender_client);
        let sender_started_on_connect = Arc::clone(&sender_started);

        let builder = ClientBuilder::new(&url)
            .auth(serde_json::json!({ "password": password }))
            .reconnect(true)
            .reconnect_on_disconnect(true)
            .on(Event::Connect, move |_, client: RawClient| {
                if let Ok(mut active_client) = sender_client_on_connect.lock() {
                    *active_client = Some(client.clone());
                }
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
                        let active_client = Arc::clone(&sender_client_for_thread);
                        thread::spawn(move || {
                            run_outgoing_sender(rx, active_client);
                        });
                    }
                }
            })
            .on(Event::Close, move |_, _| {
                if let Ok(mut active_client) = sender_client_on_close.lock() {
                    *active_client = None;
                }
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
            .on("room_created", move |payload, _| {
                let code = extract_string_field(&payload, "code");
                let member_id = extract_string_field(&payload, "member_id");
                let project_huuid = extract_string_field(&payload, "project_huuid");
                let _ = tx_room_metadata_created.send(IncomingMessage::RoomMetadata {
                    member_id,
                    project_huuid,
                    project_matches: true,
                });
                let _ = tx_room_created.send(IncomingMessage::Packet(Packet::RoomCreated { code }));
            })
            .on("room_joined", move |payload, _client: RawClient| {
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
                    let project_matches = obj["project_matches"].as_bool().unwrap_or(false);
                    let _ = tx_room_metadata_joined.send(IncomingMessage::RoomMetadata {
                        member_id,
                        project_huuid,
                        project_matches,
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
            .on("project_transfer_chunk", move |payload, _| {
                if let Some(value) = payload_to_value(&payload) {
                    let request_id = value["request_id"].as_str().unwrap_or("").to_string();
                    let index = value["index"].as_u64().unwrap_or(0) as usize;
                    let data_base64 = value["data"].as_str().unwrap_or("").to_string();
                    let _ = tx_project_transfer_chunk.send(IncomingMessage::ProjectTransferChunk {
                        request_id,
                        index,
                        data_base64,
                    });
                }
            })
            .on("project_transfer_end", move |payload, _| {
                let request_id = payload_to_value(&payload)
                    .and_then(|value| value["request_id"].as_str().map(String::from))
                    .unwrap_or_default();
                let _ = tx_project_transfer_end
                    .send(IncomingMessage::ProjectTransferEnd { request_id });
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

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| builder.connect()));
        let result = match result {
            Ok(Ok(client)) => Ok(client),
            Ok(Err(e)) => Err(format!("{e}")),
            Err(_) => Err("Connexion échouée (panic)".into()),
        };

        match result {
            Ok(client) => {
                let (audio_tx, audio_rx) = mpsc::channel();
                let audio_out_tx = out_tx.clone();
                if let Err(error) = thread::Builder::new()
                    .name("recording-audio-uploads".into())
                    .spawn(move || run_audio_sender(audio_rx, audio_out_tx))
                {
                    log::error!("cannot start the recording audio sender: {error}");
                } else {
                    self.audio_tx = Some(audio_tx);
                }
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
        let (event, payload) = packet_to_emit(packet, None);
        self.send_raw(event, payload);
    }

    /// Send a raw event via the sender thread.
    pub fn send_raw(&self, event: &str, payload: serde_json::Value) {
        log::debug!("Sending event: {event}");
        if let Some(tx) = &self.out_tx {
            let _ = tx.send(OutgoingMessage::Direct(event.to_string(), payload));
        }
    }

    pub fn send_recording_transaction(&self, transaction: &crate::recording::RecordingTransaction) {
        if let Ok(payload) = serde_json::to_value(transaction) {
            self.send_raw("recording_transaction", payload);
        }
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
            if let Some(tx) = &self.out_tx {
                let _ = tx.send(OutgoingMessage::Direct(event.to_string(), payload));
            }
            return;
        }
        if serialized.len() as u64 > crate::big_event::MAX_BIG_EVENT_BYTES {
            log::error!("{event} payload exceeds the big event size limit");
            return;
        }
        let Some(out_tx) = &self.out_tx else {
            log::warn!("dropping oversized {event}: network is not connected");
            return;
        };
        let job = BigSendJob {
            event: event.to_string(),
            serialized,
            target: target.map(str::to_owned),
        };
        if out_tx.send(OutgoingMessage::Big(job)).is_err() {
            log::error!("the network sender stopped");
        }
    }

    /// Send a full project sync, chunked when the project is large.
    pub fn send_sync(&self, payload: serde_json::Value, target: Option<&str>) {
        self.send_big_event("sync", payload, target);
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
        if let Ok(payload) = serde_json::to_value(metadata) {
            self.send_raw("project_transfer_request", payload);
        }
    }

    pub fn respond_project_transfer(&self, request_id: &str, response: &str) {
        self.send_raw(
            "project_transfer_response",
            serde_json::json!({ "request_id": request_id, "response": response }),
        );
    }

    pub fn start_project_transfer(&self, metadata: &ProjectTransferMetadata) {
        if let Ok(payload) = serde_json::to_value(metadata) {
            self.send_raw("project_transfer_start", payload);
        }
    }

    pub fn send_project_transfer_chunk(&self, request_id: &str, index: usize, data: &str) {
        self.send_raw(
            "project_transfer_chunk",
            serde_json::json!({ "request_id": request_id, "index": index, "data": data }),
        );
    }

    pub fn finish_project_transfer(&self, request_id: &str) {
        self.send_raw(
            "project_transfer_end",
            serde_json::json!({ "request_id": request_id }),
        );
    }

    pub fn report_project_transfer_loading(&self, request_id: &str) {
        self.send_raw(
            "project_transfer_loading",
            serde_json::json!({ "request_id": request_id }),
        );
    }

    pub fn report_project_transfer(&self, request_id: &str, success: bool, error: Option<&str>) {
        self.send_raw(
            "project_transfer_result",
            serde_json::json!({ "request_id": request_id, "success": success, "error": error }),
        );
    }

    pub fn send_project_file(
        &self,
        path: PathBuf,
        metadata: ProjectTransferMetadata,
    ) -> mpsc::Receiver<Result<(), String>> {
        let (result_tx, result_rx) = mpsc::channel();
        let Some(out_tx) = self.out_tx.clone() else {
            let _ = result_tx.send(Err("network is not connected".into()));
            return result_rx;
        };
        std::thread::spawn(move || {
            let result = (|| {
                let generic = crate::file_transfer::FileTransferMetadata {
                    transfer_id: metadata.request_id.clone(),
                    file_name: metadata.file_name.clone(),
                    total_bytes: metadata.total_bytes,
                    total_chunks: metadata.total_chunks,
                    chunk_size: metadata.chunk_size,
                    sha1: metadata.sha1.clone(),
                };
                generic.validate()?;
                out_tx
                    .send(OutgoingMessage::Direct(
                        "project_transfer_start".into(),
                        serde_json::to_value(&metadata).map_err(|error| error.to_string())?,
                    ))
                    .map_err(|_| "network sender stopped".to_string())?;
                for chunk in crate::file_transfer::FileChunkReader::open(&path, &generic)? {
                    let (index, data) = chunk?;
                    out_tx.send(OutgoingMessage::Direct(
                        "project_transfer_chunk".into(),
                        serde_json::json!({ "request_id": &metadata.request_id, "index": index, "data": data }),
                    )).map_err(|_| "network sender stopped".to_string())?;
                }
                out_tx
                    .send(OutgoingMessage::Direct(
                        "project_transfer_end".into(),
                        serde_json::json!({ "request_id": metadata.request_id }),
                    ))
                    .map_err(|_| "network sender stopped".to_string())?;
                Ok(())
            })();
            let _ = result_tx.send(result);
        });
        result_rx
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
        // Drop out_tx first to stop sender thread
        self.audio_tx = None;
        self.out_tx = None;
        if let Some(client) = self._client.take() {
            let _ = client.disconnect();
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
        } => {
            let mut payload = serde_json::json!({
                "code": code,
                "username": username,
                "project_huuid": project_huuid,
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
    while let Ok(job) = rx.recv() {
        let result = (|| {
            for (path, metadata) in job.files {
                metadata.validate()?;
                let start = serde_json::to_value(&metadata)
                    .map_err(|error| format!("cannot serialize FLAC metadata: {error}"))?;
                out_tx
                    .send(OutgoingMessage::Direct("audio_start".into(), start))
                    .map_err(|_| "network sender stopped".to_string())?;
                let reader = crate::audio_transfer::AudioChunkReader::open(&path, &metadata)?;
                for chunk in reader {
                    let chunk = chunk?;
                    out_tx
                        .send(OutgoingMessage::Direct(
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
                    .send(OutgoingMessage::Direct(
                        "audio_end".into(),
                        serde_json::json!({ "transfer_id": &metadata.transfer_id }),
                    ))
                    .map_err(|_| "network sender stopped".to_string())?;
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
    active_client: Arc<Mutex<Option<RawClient>>>,
) {
    use std::sync::atomic::AtomicU64;
    use std::time::{SystemTime, UNIX_EPOCH};

    static BIG_TRANSFER_COUNTER: AtomicU64 = AtomicU64::new(0);

    while let Ok(message) = rx.recv() {
        match message {
            OutgoingMessage::Direct(event, payload) => {
                emit_with_retry(&active_client, &event, &payload);
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
                    emit_with_retry(&active_client, "big_begin", &begin);
                    for (index, data) in chunks.iter().enumerate() {
                        let payload = serde_json::json!({
                            "transfer_id": transfer_id,
                            "index": index,
                            "data": data,
                        });
                        emit_with_retry(&active_client, "big_chunk", &payload);
                    }
                    let payload = serde_json::json!({ "transfer_id": transfer_id });
                    emit_with_retry(&active_client, "big_end", &payload);
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
                }
            }
        }
    }
}

fn emit_with_retry(
    active_client: &Arc<Mutex<Option<RawClient>>>,
    event: &str,
    payload: &serde_json::Value,
) {
    loop {
        let client = active_client.lock().ok().and_then(|active| active.clone());
        let Some(client) = client else {
            thread::sleep(Duration::from_millis(25));
            continue;
        };
        if client.emit(event, payload.clone()).is_ok() {
            return;
        }
        thread::sleep(Duration::from_millis(25));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

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
        }
        match receiver.recv().unwrap() {
            OutgoingMessage::Direct(event, _) => assert_eq!(event, "recording_transaction"),
            OutgoingMessage::Big(job) => panic!("received chunked event second: {}", job.event),
        }
    }
}
