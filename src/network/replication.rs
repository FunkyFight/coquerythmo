//! One bounded, coalescing mailbox per room session. Domain serialization,
//! diffing, snapshot downloads and acknowledged writes run off the UI thread.

use super::rpc::RpcClient;
use super::rythmo::{self, Changes, DirectorView, Records, RythmoDocument, Transport};
use crate::integrity::Sha1;
use crate::project::Project;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde_json::{json, Value};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

const CHUNK_BYTES: usize = 192 * 1024;
const MAX_STATE_BYTES: usize = 64 * 1024 * 1024;

pub struct ReceivedState {
    pub document: Arc<RythmoDocument>,
    pub document_revision: u64,
    pub transport: Transport,
    pub view: DirectorView,
    pub received_at: Instant,
    pub transport_age: Duration,
}

impl ReceivedState {
    pub fn frame(&self) -> i64 {
        if !self.transport.playing {
            return self.transport.frame;
        }
        let elapsed = self.transport_age + self.received_at.elapsed();
        self.transport
            .frame
            .saturating_add((elapsed.as_secs_f64() * self.transport.fps).round() as i64)
    }
}

#[derive(Default)]
struct Mailbox {
    project: Option<(Project, String)>,
    transport: Option<(Transport, Instant)>,
    view: Option<DirectorView>,
    received: Option<ReceivedState>,
    error: Option<String>,
    director: bool,
    generation: u64,
    pull: bool,
    refresh: bool,
}

#[derive(Clone)]
pub struct ReplicationSignal {
    shared: Arc<ReplicationShared>,
}

struct ReplicationShared {
    mailbox: Mutex<Mailbox>,
    wake: Condvar,
}

impl ReplicationSignal {
    pub fn admitted(&self, director: bool) {
        let mut mailbox = self.shared.mailbox.lock().unwrap();
        mailbox.director = director;
        mailbox.generation += 1;
        mailbox.pull = true;
        self.shared.wake.notify_one();
    }

    pub fn changed(&self) {
        self.shared.mailbox.lock().unwrap().pull = true;
        self.shared.wake.notify_one();
    }
}

pub struct Replicator {
    signal: ReplicationSignal,
    last_revision: Option<(u64, String, u64)>,
    last_capture: Instant,
    last_transport: Option<(Transport, Instant)>,
    last_view: Option<DirectorView>,
}

impl Replicator {
    pub fn start(rpc: RpcClient) -> Self {
        let signal = ReplicationSignal {
            shared: Arc::new(ReplicationShared {
                mailbox: Mutex::new(Mailbox::default()),
                wake: Condvar::new(),
            }),
        };
        let worker_signal = signal.clone();
        std::thread::spawn(move || run(rpc, worker_signal));
        Self {
            signal,
            last_revision: None,
            last_capture: Instant::now() - Duration::from_secs(1),
            last_transport: None,
            last_view: None,
        }
    }

    pub fn signal(&self) -> ReplicationSignal {
        self.signal.clone()
    }

    pub fn offer(
        &mut self,
        project: &Project,
        huuid: &str,
        transport: Transport,
        view: DirectorView,
    ) {
        let (director, generation) = {
            let mailbox = self.signal.shared.mailbox.lock().unwrap();
            (mailbox.director, mailbox.generation)
        };
        if !director {
            return;
        }
        let now = Instant::now();
        let revision = (project.revision(), huuid.to_owned(), generation);
        let snapshot = if self.last_revision.as_ref() != Some(&revision)
            && now.duration_since(self.last_capture) >= Duration::from_millis(33)
        {
            self.last_revision = Some(revision);
            self.last_capture = now;
            Some((project.snapshot(), huuid.to_owned()))
        } else {
            None
        };
        let send_transport = self.last_transport.as_ref().is_none_or(|(previous, when)| {
            let elapsed = now.duration_since(*when).as_secs_f64();
            let expected = previous.frame
                + if previous.playing {
                    (elapsed * previous.fps).round() as i64
                } else {
                    0
                };
            previous.playing != transport.playing
                || previous.instrumental != transport.instrumental
                || previous.rythmo != transport.rythmo
                || previous.fps != transport.fps
                || (transport.frame - expected).unsigned_abs()
                    > if transport.playing { 2 } else { 0 }
                || (transport.playing && elapsed >= 0.5)
        });
        let send_view = self.last_view.as_ref() != Some(&view);
        if snapshot.is_none() && !send_transport && !send_view {
            return;
        }
        let mut mailbox = self.signal.shared.mailbox.lock().unwrap();
        if let Some(snapshot) = snapshot {
            mailbox.project = Some(snapshot);
        }
        if send_transport {
            self.last_transport = Some((transport.clone(), now));
            mailbox.transport = Some((transport, now));
        }
        if send_view {
            self.last_view = Some(view.clone());
            mailbox.view = Some(view);
        }
        self.signal.shared.wake.notify_one();
    }

    pub fn take_received(&self) -> Option<ReceivedState> {
        self.signal.shared.mailbox.lock().unwrap().received.take()
    }
    pub fn take_error(&self) -> Option<String> {
        self.signal.shared.mailbox.lock().unwrap().error.take()
    }
    pub fn refresh(&self) {
        let mut mailbox = self.signal.shared.mailbox.lock().unwrap();
        mailbox.refresh = true;
        mailbox.pull = true;
        self.signal.shared.wake.notify_one();
    }
}

struct Replica {
    records: Records,
    desired: Records,
    revision: u64,
    generation: u64,
    document: Option<Arc<RythmoDocument>>,
    document_revision: u64,
    last_pull: Instant,
}

fn run(rpc: RpcClient, signal: ReplicationSignal) {
    let mut replica = Replica {
        records: Records::new(),
        desired: Records::new(),
        revision: 0,
        generation: 0,
        document: None,
        document_revision: 0,
        last_pull: Instant::now() - Duration::from_secs(2),
    };
    while !rpc.is_stopped() {
        let (project, transport, view, director, generation, pull, refresh) = {
            let mailbox = signal.shared.mailbox.lock().unwrap();
            let (mut mailbox, _) = signal
                .shared
                .wake
                .wait_timeout(mailbox, Duration::from_millis(33))
                .unwrap();
            if !rpc.is_ready() {
                continue;
            }
            (
                mailbox.project.take(),
                mailbox.transport.take(),
                mailbox.view.take(),
                mailbox.director,
                mailbox.generation,
                std::mem::take(&mut mailbox.pull),
                std::mem::take(&mut mailbox.refresh),
            )
        };
        let result = (|| {
            if replica.generation != generation {
                let info = rpc.call("state_info", json!({}))?;
                replica.revision = if director { revision(&info)? } else { 0 };
                replica.records.clear();
                replica.document = None;
                replica.generation = generation;
            }
            if director {
                if let Some((project, huuid)) = project {
                    let mut desired = rythmo::document_records(&project, &huuid)?;
                    for key in ["transport", "view"] {
                        if let Some(value) = replica.desired.get(key) {
                            desired.insert(key.into(), value.clone());
                        }
                    }
                    replica.desired = desired;
                }
                if let Some((mut transport, captured)) = transport {
                    if transport.playing {
                        transport.frame = transport.frame.saturating_add(
                            (captured.elapsed().as_secs_f64() * transport.fps).round() as i64,
                        );
                    }
                    replica.desired.insert(
                        "transport".into(),
                        serde_json::to_value(transport).map_err(|e| e.to_string())?,
                    );
                }
                if let Some(view) = view {
                    replica.desired.insert(
                        "view".into(),
                        serde_json::to_value(view).map_err(|e| e.to_string())?,
                    );
                }
                if replica.desired.contains_key("manifest")
                    && replica.desired.contains_key("transport")
                    && replica.desired.contains_key("view")
                {
                    publish(&rpc, &mut replica)?;
                }
            } else if pull || refresh || replica.last_pull.elapsed() >= Duration::from_secs(1) {
                if refresh {
                    replica.records.clear();
                    replica.document = None;
                    replica.revision = 0;
                }
                if let Some(received) = pull_state(&rpc, &mut replica)? {
                    signal.shared.mailbox.lock().unwrap().received = Some(received);
                }
                replica.last_pull = Instant::now();
            }
            Ok::<_, String>(())
        })();
        if let Err(error) = result {
            if error == "state_revision_conflict" {
                replica.generation = 0;
            } else {
                signal.shared.mailbox.lock().unwrap().error = Some(error);
            }
            // A permanent validation failure must not spin at frame rate.
            let guard = signal.shared.mailbox.lock().unwrap();
            let _ = signal
                .shared
                .wake
                .wait_timeout(guard, Duration::from_secs(1));
        }
    }
}

fn revision(reply: &Value) -> Result<u64, String> {
    reply["revision"]
        .as_u64()
        .ok_or_else(|| "invalid state revision".into())
}

fn publish(rpc: &RpcClient, replica: &mut Replica) -> Result<(), String> {
    let changes = rythmo::diff_records(&replica.records, &replica.desired);
    if changes.is_empty() {
        return Ok(());
    }
    let update_id = format!("state_{:032x}", rand::random::<u128>());
    let patch = json!({ "update_id": update_id, "base_revision": replica.revision,
        "replace": replica.records.is_empty(), "changes": changes });
    let serialized = serde_json::to_vec(&patch).map_err(|e| e.to_string())?;
    if serialized.len() > MAX_STATE_BYTES {
        return Err("bande rythmo exceeds the synchronization size limit".into());
    }
    let reply = if serialized.len() <= CHUNK_BYTES {
        rpc.call("state_publish", patch)?
    } else {
        let mut digest = Sha1::new();
        digest.update(&serialized);
        let begin = rpc.call(
            "state_upload_begin",
            json!({ "update_id": update_id,
            "total_bytes": serialized.len(), "sha1": digest.finalize_hex() }),
        )?;
        if begin["complete"] == true {
            begin
        } else {
            let next = begin["next_index"]
                .as_u64()
                .ok_or("invalid state upload acknowledgement")? as usize;
            if next > serialized.len().div_ceil(CHUNK_BYTES) {
                return Err("invalid state upload offset".into());
            }
            for (index, chunk) in serialized.chunks(CHUNK_BYTES).enumerate().skip(next) {
                rpc.call("state_upload_chunk", json!({ "update_id": update_id, "index": index, "data": STANDARD.encode(chunk) }))?;
            }
            rpc.call("state_upload_commit", json!({ "update_id": update_id }))?
        }
    };
    replica.revision = revision(&reply)?;
    replica.records = replica.desired.clone();
    Ok(())
}

fn pull_state(rpc: &RpcClient, replica: &mut Replica) -> Result<Option<ReceivedState>, String> {
    let started = Instant::now();
    let mut reply = rpc.call("state_pull", json!({ "since": replica.revision }))?;
    if reply["snapshot_required"] != true
        && revision(&reply)? == replica.revision
        && replica.document.is_some()
    {
        return Ok(None);
    }
    let mut reply_at = Instant::now();
    let mut candidate = replica.records.clone();
    let mut document_changed = replica.document.is_none();
    if reply["snapshot_required"] == true {
        reply = rpc.call("state_snapshot_begin", json!({}))?;
        reply_at = Instant::now();
        candidate = read_snapshot(rpc, &reply)?;
        document_changed = true;
    } else {
        if revision(&reply)? == replica.revision && replica.document.is_some() {
            return Ok(None);
        }
        let changes: Changes =
            serde_json::from_value(reply["changes"].clone()).map_err(|e| e.to_string())?;
        document_changed |= changes
            .keys()
            .any(|key| !matches!(key.as_str(), "view" | "transport"));
        if reply["replace"] == true {
            candidate.clear();
        }
        rythmo::apply_changes(&mut candidate, changes);
    }
    if !candidate.contains_key("manifest") {
        return Ok(None);
    }
    let document = if document_changed {
        Arc::new(rythmo::decode_document(&candidate)?)
    } else {
        replica.document.as_ref().unwrap().clone()
    };
    let transport: Transport = serde_json::from_value(
        candidate
            .get("transport")
            .cloned()
            .ok_or("missing transport record")?,
    )
    .map_err(|e| e.to_string())?;
    if !transport.fps.is_finite()
        || transport.fps <= 0.0
        || transport.fps > 240.0
        || transport.frame < 0
    {
        return Err("invalid playback state".into());
    }
    let view: DirectorView = serde_json::from_value(
        candidate
            .get("view")
            .cloned()
            .ok_or("missing view record")?,
    )
    .map_err(|e| e.to_string())?;
    replica.revision = revision(&reply)?;
    if document_changed {
        replica.document_revision = replica.revision;
    }
    replica.records = candidate;
    replica.document = Some(document.clone());
    let server_age = reply["server_time"]
        .as_u64()
        .unwrap_or(0)
        .saturating_sub(reply["transport_time"].as_u64().unwrap_or(0));
    Ok(Some(ReceivedState {
        document,
        document_revision: replica.document_revision,
        transport,
        view,
        received_at: Instant::now(),
        transport_age: Duration::from_millis(server_age)
            + reply_at.duration_since(started).min(Duration::from_secs(2)) / 2
            + reply_at.elapsed(),
    }))
}

fn read_snapshot(rpc: &RpcClient, info: &Value) -> Result<Records, String> {
    let id = info["snapshot_id"].as_str().ok_or("missing snapshot id")?;
    let total_bytes = info["total_bytes"]
        .as_u64()
        .ok_or("invalid snapshot size")? as usize;
    let total_chunks = info["total_chunks"]
        .as_u64()
        .ok_or("invalid snapshot chunks")? as usize;
    if total_bytes > MAX_STATE_BYTES || total_chunks != total_bytes.div_ceil(CHUNK_BYTES) {
        return Err("invalid snapshot geometry".into());
    }
    let result = (|| {
        let mut bytes = Vec::with_capacity(total_bytes);
        for index in 0..total_chunks {
            let reply = rpc.call(
                "state_snapshot_read",
                json!({ "snapshot_id": id, "index": index }),
            )?;
            if reply["index"].as_u64() != Some(index as u64) {
                return Err("invalid snapshot chunk order".into());
            }
            let data = STANDARD
                .decode(reply["data"].as_str().ok_or("invalid snapshot chunk")?)
                .map_err(|e| e.to_string())?;
            if data.len() != (total_bytes - bytes.len()).min(CHUNK_BYTES) {
                return Err("invalid snapshot chunk size".into());
            }
            bytes.extend_from_slice(&data);
        }
        let mut digest = Sha1::new();
        digest.update(&bytes);
        if info["sha1"].as_str() != Some(digest.finalize_hex().as_str()) {
            return Err("snapshot checksum mismatch".into());
        }
        serde_json::from_slice(&bytes).map_err(|e| e.to_string())
    })();
    let _ = rpc.call("state_snapshot_end", json!({ "snapshot_id": id }));
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_burst_of_edits_keeps_only_the_latest_project_in_the_mailbox() {
        let rpc = RpcClient::new();
        let mut replication = Replicator::start(rpc.clone());
        replication.signal.admitted(true);
        let mut project = Project::new();
        let line = project.add_line(0, 40, 0.2);
        for text in ["one", "two", "latest"] {
            project.get_line_mut(line).unwrap().text = text.into();
            replication.last_capture = Instant::now() - Duration::from_secs(1);
            replication.offer(
                &project,
                "project",
                Transport {
                    frame: 0,
                    playing: false,
                    fps: 24.0,
                    instrumental: false,
                    rythmo: true,
                },
                DirectorView {
                    selection: None,
                    compact_empty_tracks: false,
                    active_stroke: None,
                    font_family: "sans-serif".into(),
                },
            );
        }
        let mailbox = replication.signal.shared.mailbox.lock().unwrap();
        assert_eq!(
            mailbox
                .project
                .as_ref()
                .unwrap()
                .0
                .get_line(line)
                .unwrap()
                .text,
            "latest"
        );
        rpc.stop();
    }
}
