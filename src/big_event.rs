//! Chunked `big_*` event channel for payloads too large for one websocket
//! frame (tungstenite drops frames above 16 MiB on the receiving client).
//!
//! Payloads at or below [`BIG_EVENT_DIRECT_MAX_BYTES`] keep using the legacy
//! single-frame events; larger ones are framed as `big_begin` / `big_chunk`* /
//! `big_end`. The server relays frames without reassembling them; senders and
//! receivers here own framing, ordering and SHA-1 integrity, mirroring
//! `audio_transfer` and `file_transfer`.

use std::collections::HashMap;

use base64::{engine::general_purpose::STANDARD, Engine as _};

use crate::integrity::Sha1;

pub const BIG_EVENT_CHUNK_BYTES: usize = 256 * 1024;
pub const BIG_EVENT_DIRECT_MAX_BYTES: usize = 256 * 1024;
pub const MAX_BIG_EVENT_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const BIG_EVENTS: [&str; 2] = ["sync", "recording_prepare"];

/// Announced geometry and integrity of one chunked event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BigEventBegin {
    pub transfer_id: String,
    pub event: String,
    pub total_bytes: u64,
    pub total_chunks: usize,
    pub chunk_size: usize,
    pub sha1: String,
}

impl BigEventBegin {
    fn validate(&self) -> Result<(), String> {
        if self.transfer_id.is_empty()
            || self.transfer_id.len() > 96
            || !self
                .transfer_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err("invalid big transfer id".into());
        }
        if !BIG_EVENTS.contains(&self.event.as_str()) {
            return Err("invalid big transfer event".into());
        }
        if self.total_bytes == 0 || self.total_bytes > MAX_BIG_EVENT_BYTES {
            return Err("big transfer size is invalid".into());
        }
        if self.chunk_size == 0 || self.chunk_size > BIG_EVENT_CHUNK_BYTES {
            return Err("big transfer chunk size is invalid".into());
        }
        let expected = self.total_bytes.div_ceil(self.chunk_size as u64) as usize;
        if self.total_chunks != expected {
            return Err("big transfer chunk count is invalid".into());
        }
        if self.sha1.len() != 40
            || !self
                .sha1
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err("big transfer SHA-1 is invalid".into());
        }
        Ok(())
    }
}

/// Frame a serialized payload into the begin geometry and its ordered base64
/// chunks. Callers must reject payloads above [`MAX_BIG_EVENT_BYTES`] first.
pub fn frame_big_event(serialized: &[u8]) -> Result<(BigEventFrame, Vec<String>), String> {
    let total_bytes = u64::try_from(serialized.len())
        .map_err(|_| "big event payload is too large".to_string())?;
    if total_bytes == 0 || total_bytes > MAX_BIG_EVENT_BYTES {
        return Err("big event payload size is outside the supported range".into());
    }
    let mut digest = Sha1::new();
    digest.update(serialized);
    let frame = BigEventFrame {
        total_bytes,
        total_chunks: serialized.len().div_ceil(BIG_EVENT_CHUNK_BYTES),
        chunk_size: BIG_EVENT_CHUNK_BYTES,
        sha1: digest.finalize_hex(),
    };
    let chunks = serialized
        .chunks(BIG_EVENT_CHUNK_BYTES)
        .map(|chunk| STANDARD.encode(chunk))
        .collect();
    Ok((frame, chunks))
}

/// Geometry shared by the begin frame and the chunks, without the routing
/// fields (`transfer_id`, `event`) the caller already knows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BigEventFrame {
    pub total_bytes: u64,
    pub total_chunks: usize,
    pub chunk_size: usize,
    pub sha1: String,
}

struct ActiveBigEvent {
    event: String,
    buffer: Vec<u8>,
    next_index: usize,
    total_chunks: usize,
    total_bytes: u64,
    chunk_size: usize,
    sha1: String,
}

/// Reassembles concurrent chunked events. Chunks must arrive in order, which
/// Socket.IO guarantees per sender; anything else is rejected as a protocol
/// error instead of corrupting the project state.
#[derive(Default)]
pub struct BigEventReceiver {
    active: HashMap<String, ActiveBigEvent>,
}

impl BigEventReceiver {
    pub fn begin(&mut self, begin: BigEventBegin) -> Result<(), String> {
        begin.validate()?;
        if self.active.contains_key(&begin.transfer_id) {
            return Err("duplicate big transfer id".into());
        }
        self.active.insert(
            begin.transfer_id,
            ActiveBigEvent {
                event: begin.event,
                buffer: Vec::new(),
                next_index: 0,
                total_chunks: begin.total_chunks,
                total_bytes: begin.total_bytes,
                chunk_size: begin.chunk_size,
                sha1: begin.sha1,
            },
        );
        Ok(())
    }

    pub fn push_base64(
        &mut self,
        transfer_id: &str,
        index: usize,
        data_base64: &str,
    ) -> Result<(), String> {
        let Some(transfer) = self.active.get_mut(transfer_id) else {
            return Err("unknown big transfer".to_string());
        };
        // Any protocol error aborts the transfer: the sender restarts with a
        // fresh big_begin instead of resuming from ambiguous state.
        let result = (|| {
            if index != transfer.next_index {
                return Err(format!(
                    "out-of-order big chunk: expected {}, received {index}",
                    transfer.next_index
                ));
            }
            let bytes = STANDARD
                .decode(data_base64)
                .map_err(|error| format!("invalid big chunk: {error}"))?;
            if bytes.is_empty() || bytes.len() > transfer.chunk_size {
                return Err("big chunk size is invalid".to_string());
            }
            if STANDARD.encode(&bytes) != data_base64 {
                return Err("big chunk is not canonical base64".to_string());
            }
            let received = (transfer.buffer.len() as u64).saturating_add(bytes.len() as u64);
            if received > transfer.total_bytes {
                return Err("big transfer exceeds its announced size".to_string());
            }
            transfer.buffer.extend_from_slice(&bytes);
            transfer.next_index += 1;
            Ok(())
        })();
        if result.is_err() {
            self.active.remove(transfer_id);
        }
        result
    }

    /// Complete a transfer and return its event name and reassembled payload.
    pub fn finish(&mut self, transfer_id: &str) -> Result<(String, Vec<u8>), String> {
        let transfer = self
            .active
            .remove(transfer_id)
            .ok_or_else(|| "unknown big transfer".to_string())?;
        if transfer.next_index != transfer.total_chunks
            || transfer.buffer.len() as u64 != transfer.total_bytes
        {
            return Err("big transfer ended before completion".into());
        }
        let mut digest = Sha1::new();
        digest.update(&transfer.buffer);
        if digest.finalize_hex() != transfer.sha1 {
            return Err("big transfer SHA-1 mismatch".into());
        }
        Ok((transfer.event, transfer.buffer))
    }

    pub fn clear(&mut self) {
        self.active.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn begin_for(transfer_id: &str, event: &str, frame: &BigEventFrame) -> BigEventBegin {
        BigEventBegin {
            transfer_id: transfer_id.to_string(),
            event: event.to_string(),
            total_bytes: frame.total_bytes,
            total_chunks: frame.total_chunks,
            chunk_size: frame.chunk_size,
            sha1: frame.sha1.clone(),
        }
    }

    #[test]
    fn chunked_big_event_round_trip_restores_the_payload() {
        let payload: Vec<u8> = (0..(BIG_EVENT_CHUNK_BYTES * 2 + 17))
            .map(|index| (index % 251) as u8)
            .collect();
        let (frame, chunks) = frame_big_event(&payload).unwrap();
        assert_eq!(chunks.len(), 3);

        let mut receiver = BigEventReceiver::default();
        receiver.begin(begin_for("big_1", "sync", &frame)).unwrap();
        for (index, chunk) in chunks.iter().enumerate() {
            receiver.push_base64("big_1", index, chunk).unwrap();
        }
        let (event, reassembled) = receiver.finish("big_1").unwrap();
        assert_eq!(event, "sync");
        assert_eq!(reassembled, payload);
    }

    #[test]
    fn receiver_rejects_out_of_order_unknown_and_tampered_frames() {
        let payload = b"{\"project\":{}}".to_vec();
        let (frame, chunks) = frame_big_event(&payload).unwrap();

        let mut receiver = BigEventReceiver::default();
        // Unknown transfer ids are rejected outright.
        assert!(receiver.push_base64("big_1", 0, &chunks[0]).is_err());
        assert!(receiver.finish("big_1").is_err());

        // An out-of-order chunk aborts the transfer.
        receiver.begin(begin_for("big_1", "sync", &frame)).unwrap();
        assert!(receiver.push_base64("big_1", 1, &chunks[0]).is_err());
        assert!(receiver.push_base64("big_1", 0, &chunks[0]).is_err());

        // A tampered digest passes chunk checks but fails SHA-1 at finish.
        let mut forged = begin_for("big_2", "sync", &frame);
        forged.sha1 = "0".repeat(40);
        receiver.begin(forged).unwrap();
        receiver.push_base64("big_2", 0, &chunks[0]).unwrap();
        assert!(receiver.finish("big_2").is_err());

        // Unknown events are rejected at begin.
        let unknown_event = begin_for("big_3", "recording_view", &frame);
        assert!(receiver.begin(unknown_event).is_err());
    }

    #[test]
    fn begin_rejects_inconsistent_geometry_and_duplicate_ids() {
        let payload = vec![7_u8; BIG_EVENT_CHUNK_BYTES + 1];
        let (frame, _) = frame_big_event(&payload).unwrap();
        let mut receiver = BigEventReceiver::default();
        let begin = begin_for("big_1", "recording_prepare", &frame);
        receiver.begin(begin.clone()).unwrap();
        assert!(receiver.begin(begin).is_err());

        let mut bad = begin_for("big_2", "sync", &frame);
        bad.total_chunks = 1;
        assert!(receiver.begin(bad).is_err());
        let mut bad = begin_for("big_2", "sync", &frame);
        bad.chunk_size = BIG_EVENT_CHUNK_BYTES + 1;
        assert!(receiver.begin(bad).is_err());
        let mut bad = begin_for("big_2", "sync", &frame);
        bad.sha1 = "XYZ".into();
        assert!(receiver.begin(bad).is_err());
    }
}
