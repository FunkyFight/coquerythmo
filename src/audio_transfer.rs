//! Bounded, integrity-checked FLAC transfer primitives.
//!
//! Socket.IO transport lives in `network`; this module owns file framing and
//! validation so large takes are never held in memory as one base64 value.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};

use crate::integrity::Sha1;
use crate::recording::{AudioAsset, AudioClipId, AudioTrackId, CaptureTarget, RecordedAudio};

pub const AUDIO_CHUNK_BYTES: usize = 192 * 1024;
pub const MAX_AUDIO_BYTES: u64 = 4 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioTransferMetadata {
    pub transfer_id: String,
    pub file_name: String,
    pub total_bytes: u64,
    pub total_chunks: usize,
    pub chunk_size: usize,
    pub sha1: String,
    pub target: CaptureTarget,
    pub audio: RecordedAudio,
    /// False for an asset whose timeline transaction was sent separately
    /// (imports and catch-up transfers). The explicit flag keeps the wire
    /// shape compatible with clients that still require `target`.
    #[serde(default = "default_commit_on_receive")]
    pub commit_on_receive: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_member_id: Option<String>,
    /// Restrict a catch-up transfer to one member. Live takes and imports use
    /// `None` and are relayed to every other participant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_member_id: Option<String>,
}

impl AudioTransferMetadata {
    pub fn from_file(
        transfer_id: impl Into<String>,
        path: &Path,
        target: CaptureTarget,
        audio: RecordedAudio,
    ) -> Result<Self, String> {
        Self::from_descriptor_file(transfer_id, path, target, audio, true, None)
    }

    pub fn from_asset_file(
        transfer_id: impl Into<String>,
        path: &Path,
        asset: &AudioAsset,
        to_member_id: Option<String>,
    ) -> Result<Self, String> {
        let audio = RecordedAudio {
            file_name: asset.file_name.clone(),
            sample_rate: asset.sample_rate,
            channels: asset.channels,
            sample_count: asset.sample_count,
            checksum: asset.checksum.clone(),
            waveform: asset.waveform.clone(),
        };
        let compatibility_target = CaptureTarget {
            track_id: AudioTrackId::new(0),
            asset_id: asset.id,
            clip_id: AudioClipId::new(0),
            start_frame: 0,
        };
        let metadata = Self::from_descriptor_file(
            transfer_id,
            path,
            compatibility_target,
            audio,
            false,
            to_member_id,
        )?;
        if metadata.sha1 != asset.checksum {
            return Err("recording asset FLAC checksum no longer matches the timeline".into());
        }
        Ok(metadata)
    }

    fn from_descriptor_file(
        transfer_id: impl Into<String>,
        path: &Path,
        target: CaptureTarget,
        mut audio: RecordedAudio,
        commit_on_receive: bool,
        to_member_id: Option<String>,
    ) -> Result<Self, String> {
        let transfer_id = transfer_id.into();
        validate_transfer_id(&transfer_id).and_then(|_| {
            let file_name = portable_flac_name(path)?;
            let (total_bytes, sha1) = hash_file(path)?;
            if total_bytes == 0 || total_bytes > MAX_AUDIO_BYTES {
                return Err("FLAC transfer size is outside the supported range".into());
            }
            let total_chunks = usize::try_from(total_bytes.div_ceil(AUDIO_CHUNK_BYTES as u64))
                .map_err(|_| "FLAC transfer contains too many chunks".to_string())?;
            audio.file_name = file_name.clone();
            audio.checksum = sha1.clone();
            let metadata = Self {
                transfer_id,
                file_name,
                total_bytes,
                total_chunks,
                chunk_size: AUDIO_CHUNK_BYTES,
                sha1,
                target,
                audio,
                commit_on_receive,
                from_member_id: None,
                to_member_id,
            };
            metadata.validate()?;
            Ok(metadata)
        })
    }

    pub fn validate(&self) -> Result<(), String> {
        validate_transfer_id(&self.transfer_id)?;
        validate_flac_leaf(&self.file_name)?;
        if self.total_bytes == 0 || self.total_bytes > MAX_AUDIO_BYTES {
            return Err("FLAC transfer size is outside the supported range".into());
        }
        if self.chunk_size == 0 || self.chunk_size > AUDIO_CHUNK_BYTES {
            return Err("invalid FLAC chunk size".into());
        }
        let expected_chunks = usize::try_from(
            self.total_bytes
                .div_ceil(u64::try_from(self.chunk_size).unwrap_or(u64::MAX)),
        )
        .map_err(|_| "FLAC transfer contains too many chunks".to_string())?;
        if self.total_chunks != expected_chunks {
            return Err("FLAC chunk count does not match its byte size".into());
        }
        if self.sha1.len() != 40
            || !self
                .sha1
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err("invalid FLAC SHA-1 digest".into());
        }
        if self.audio.file_name != self.file_name || self.audio.checksum != self.sha1 {
            return Err("FLAC descriptor does not match transfer metadata".into());
        }
        if self.from_member_id.as_ref().is_some_and(|member_id| {
            member_id.is_empty() || member_id.len() > 128 || member_id.chars().any(char::is_control)
        }) {
            return Err("invalid FLAC sender id".into());
        }
        if self.to_member_id.as_ref().is_some_and(|member_id| {
            member_id.is_empty() || member_id.len() > 128 || member_id.chars().any(char::is_control)
        }) {
            return Err("invalid FLAC recipient id".into());
        }
        Ok(())
    }

    pub fn prefix_file_name_with_user(&mut self, username: &str) -> Result<(), String> {
        let safe_username: String = username
            .chars()
            .map(|character| {
                if character.is_control()
                    || matches!(
                        character,
                        '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
                    )
                {
                    '_'
                } else {
                    character
                }
            })
            .collect();
        let safe_username = safe_username.trim().trim_matches(['.', ' ']);
        let safe_username = if safe_username.is_empty() {
            "user"
        } else {
            safe_username
        };
        let prefix: String = safe_username.chars().take(80).collect();
        if !self.file_name.starts_with(&format!("{prefix}_")) {
            let stem = self
                .file_name
                .strip_suffix(".flac")
                .or_else(|| self.file_name.strip_suffix(".FLAC"))
                .unwrap_or("take");
            let available_stem = 256usize.saturating_sub(prefix.chars().count() + 6);
            let stem: String = stem.chars().take(available_stem).collect();
            self.file_name = format!("{prefix}_{stem}.flac");
        }
        self.audio.file_name = self.file_name.clone();
        self.validate()
    }
}

fn default_commit_on_receive() -> bool {
    true
}

#[derive(Debug)]
pub struct AudioChunk {
    pub index: usize,
    pub data_base64: String,
}

pub struct AudioChunkReader {
    file: File,
    index: usize,
    expected_chunks: usize,
    chunk_size: usize,
}

impl AudioChunkReader {
    pub fn open(path: &Path, metadata: &AudioTransferMetadata) -> Result<Self, String> {
        metadata.validate()?;
        let file = File::open(path)
            .map_err(|error| format!("cannot open FLAC {}: {error}", path.display()))?;
        Ok(Self {
            file,
            index: 0,
            expected_chunks: metadata.total_chunks,
            chunk_size: metadata.chunk_size,
        })
    }
}

impl Iterator for AudioChunkReader {
    type Item = Result<AudioChunk, String>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.expected_chunks {
            return None;
        }
        let mut bytes = vec![0_u8; self.chunk_size];
        let read = match self.file.read(&mut bytes) {
            Ok(read) if read > 0 => read,
            Ok(_) => return Some(Err("FLAC ended before the announced chunk count".into())),
            Err(error) => return Some(Err(format!("cannot read FLAC chunk: {error}"))),
        };
        bytes.truncate(read);
        let chunk = AudioChunk {
            index: self.index,
            data_base64: STANDARD.encode(bytes),
        };
        self.index += 1;
        Some(Ok(chunk))
    }
}

#[derive(Debug)]
pub struct ReceivedAudio {
    pub metadata: AudioTransferMetadata,
    pub path: PathBuf,
}

struct ActiveTransfer {
    metadata: AudioTransferMetadata,
    temporary_path: PathBuf,
    final_path: PathBuf,
    file: File,
    digest: Sha1,
    next_index: usize,
    received_bytes: u64,
}

/// Receives several interleaved transfers while enforcing sequential chunks
/// inside each transfer. Files become visible only after size and SHA-1 match.
#[derive(Default)]
pub struct AudioTransferReceiver {
    active: BTreeMap<String, ActiveTransfer>,
}

impl AudioTransferReceiver {
    pub fn begin(
        &mut self,
        metadata: AudioTransferMetadata,
        destination_dir: &Path,
    ) -> Result<(), String> {
        metadata.validate()?;
        if self.active.contains_key(&metadata.transfer_id) {
            return Err("duplicate FLAC transfer id".into());
        }
        fs::create_dir_all(destination_dir).map_err(|error| {
            format!(
                "cannot create FLAC receive directory {}: {error}",
                destination_dir.display()
            )
        })?;
        let safe_name = metadata.file_name.clone();
        let final_path = destination_dir.join(&safe_name);
        let temporary_path = destination_dir.join(format!(".{safe_name}.part"));
        if final_path.exists() || temporary_path.exists() {
            return Err("FLAC transfer destination already exists".into());
        }
        let file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary_path)
            .map_err(|error| format!("cannot create FLAC transfer file: {error}"))?;
        self.active.insert(
            metadata.transfer_id.clone(),
            ActiveTransfer {
                metadata,
                temporary_path,
                final_path,
                file,
                digest: Sha1::new(),
                next_index: 0,
                received_bytes: 0,
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
        let transfer = self
            .active
            .get_mut(transfer_id)
            .ok_or_else(|| "unknown FLAC transfer".to_string())?;
        if index != transfer.next_index {
            return Err(format!(
                "out-of-order FLAC chunk: expected {}, received {index}",
                transfer.next_index
            ));
        }
        let maximum_encoded_len = transfer.metadata.chunk_size.div_ceil(3).saturating_mul(4);
        if data_base64.len() > maximum_encoded_len {
            return Err("FLAC base64 chunk exceeds its announced bound".into());
        }
        let bytes = STANDARD
            .decode(data_base64)
            .map_err(|error| format!("invalid FLAC base64 chunk: {error}"))?;
        if STANDARD.encode(&bytes) != data_base64 {
            return Err("FLAC base64 chunk is not canonical".into());
        }
        if bytes.is_empty() || bytes.len() > transfer.metadata.chunk_size {
            return Err("FLAC chunk size is invalid".into());
        }
        let received_bytes = transfer
            .received_bytes
            .saturating_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX));
        if received_bytes > transfer.metadata.total_bytes {
            return Err("FLAC transfer exceeds announced size".into());
        }
        transfer
            .file
            .write_all(&bytes)
            .map_err(|error| format!("cannot write FLAC chunk: {error}"))?;
        transfer.digest.update(&bytes);
        transfer.received_bytes = received_bytes;
        transfer.next_index += 1;
        Ok(())
    }

    pub fn finish(&mut self, transfer_id: &str) -> Result<ReceivedAudio, String> {
        let mut transfer = self
            .active
            .remove(transfer_id)
            .ok_or_else(|| "unknown FLAC transfer".to_string())?;
        let result = (|| {
            if transfer.next_index != transfer.metadata.total_chunks
                || transfer.received_bytes != transfer.metadata.total_bytes
            {
                return Err("FLAC transfer ended before all bytes arrived".into());
            }
            transfer
                .file
                .flush()
                .map_err(|error| format!("cannot flush FLAC transfer: {error}"))?;
            transfer
                .file
                .sync_all()
                .map_err(|error| format!("cannot sync FLAC transfer: {error}"))?;
            let digest = transfer.digest.clone().finalize_hex();
            if digest != transfer.metadata.sha1 {
                return Err("FLAC transfer SHA-1 mismatch".into());
            }
            drop(transfer.file);
            fs::rename(&transfer.temporary_path, &transfer.final_path)
                .map_err(|error| format!("cannot finalize FLAC transfer: {error}"))?;
            Ok(ReceivedAudio {
                metadata: transfer.metadata.clone(),
                path: transfer.final_path.clone(),
            })
        })();
        if result.is_err() {
            let _ = fs::remove_file(&transfer.temporary_path);
        }
        result
    }

    pub fn cancel(&mut self, transfer_id: &str) -> bool {
        let Some(transfer) = self.active.remove(transfer_id) else {
            return false;
        };
        drop(transfer.file);
        let _ = fs::remove_file(transfer.temporary_path);
        true
    }
}

impl Drop for AudioTransferReceiver {
    fn drop(&mut self) {
        for (_, transfer) in std::mem::take(&mut self.active) {
            drop(transfer.file);
            let _ = fs::remove_file(transfer.temporary_path);
        }
    }
}

fn hash_file(path: &Path) -> Result<(u64, String), String> {
    let mut file = File::open(path)
        .map_err(|error| format!("cannot open FLAC {}: {error}", path.display()))?;
    let mut digest = Sha1::new();
    let mut total = 0_u64;
    let mut buffer = vec![0_u8; AUDIO_CHUNK_BYTES];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("cannot hash FLAC {}: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
        total = total.saturating_add(u64::try_from(read).unwrap_or(u64::MAX));
        if total > MAX_AUDIO_BYTES {
            return Err("FLAC transfer exceeds the supported size".into());
        }
    }
    Ok((total, digest.finalize_hex()))
}

fn portable_flac_name(path: &Path) -> Result<String, String> {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .ok_or_else(|| "FLAC path has no file name".to_string())?;
    validate_flac_leaf(&name)?;
    Ok(name)
}

fn validate_flac_leaf(name: &str) -> Result<(), String> {
    if name.trim().is_empty()
        || name.trim() != name
        || name.chars().any(char::is_control)
        || name
            .chars()
            .any(|character| matches!(character, '/' | '\\' | ':'))
        || matches!(name, "." | "..")
        || !name
            .rsplit_once('.')
            .is_some_and(|(_, extension)| extension.eq_ignore_ascii_case("flac"))
    {
        return Err("recorded audio must be a portable .flac file name".into());
    }
    Ok(())
}

fn validate_transfer_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id.len() > 96
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err("invalid FLAC transfer id".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recording::{AudioAssetId, AudioClipId, AudioTrackId, WaveformData};

    fn target() -> CaptureTarget {
        CaptureTarget {
            track_id: AudioTrackId::new(1),
            asset_id: AudioAssetId::new(2),
            clip_id: AudioClipId::new(3),
            start_frame: 48,
        }
    }

    fn recorded() -> RecordedAudio {
        RecordedAudio {
            file_name: "take.flac".into(),
            sample_rate: 48_000,
            channels: 1,
            sample_count: 48_000,
            checksum: "placeholder".into(),
            waveform: WaveformData::new(480, vec![0.5]).unwrap(),
        }
    }

    #[test]
    fn chunked_roundtrip_is_atomic_and_integrity_checked() {
        let root = std::env::temp_dir().join(format!(
            "coquerythmo-audio-transfer-{}-roundtrip",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let source = root.join("take.flac");
        fs::write(&source, vec![0x5a; AUDIO_CHUNK_BYTES + 17]).unwrap();
        let metadata =
            AudioTransferMetadata::from_file("take_1", &source, target(), recorded()).unwrap();
        let mut receiver = AudioTransferReceiver::default();
        let received_dir = root.join("received");
        receiver.begin(metadata.clone(), &received_dir).unwrap();
        for chunk in AudioChunkReader::open(&source, &metadata).unwrap() {
            let chunk = chunk.unwrap();
            receiver
                .push_base64(&metadata.transfer_id, chunk.index, &chunk.data_base64)
                .unwrap();
        }
        let received = receiver.finish(&metadata.transfer_id).unwrap();
        assert_eq!(
            received.path.file_name().and_then(|name| name.to_str()),
            Some("take.flac")
        );
        assert_eq!(fs::read(received.path).unwrap(), fs::read(source).unwrap());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn authenticated_username_prefix_is_idempotent() {
        let root = std::env::temp_dir().join(format!(
            "coquerythmo-audio-transfer-prefix-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let source = root.join("take.flac");
        fs::write(&source, b"audio").unwrap();
        let mut metadata =
            AudioTransferMetadata::from_file("take_3", &source, target(), recorded()).unwrap();

        metadata.prefix_file_name_with_user("Comé/dien").unwrap();
        metadata.prefix_file_name_with_user("Comé/dien").unwrap();

        assert_eq!(metadata.file_name, "Comé_dien_take.flac");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn asset_publication_is_targeted_without_requesting_a_new_clip() {
        let root = std::env::temp_dir().join(format!(
            "coquerythmo-audio-transfer-publication-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let source = root.join("take.flac");
        fs::write(&source, b"audio").unwrap();
        let capture =
            AudioTransferMetadata::from_file("take_4", &source, target(), recorded()).unwrap();
        let asset = capture.audio.clone().into_asset(AudioAssetId::new(9));

        let publication = AudioTransferMetadata::from_asset_file(
            "asset_9",
            &source,
            &asset,
            Some("actor-2".into()),
        )
        .unwrap();

        assert!(!publication.commit_on_receive);
        assert_eq!(publication.target.asset_id, asset.id);
        assert_eq!(publication.to_member_id.as_deref(), Some("actor-2"));

        let mut legacy_json = serde_json::to_value(capture).unwrap();
        legacy_json
            .as_object_mut()
            .unwrap()
            .remove("commit_on_receive");
        let legacy: AudioTransferMetadata = serde_json::from_value(legacy_json).unwrap();
        assert!(legacy.commit_on_receive);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn receiver_rejects_out_of_order_and_tampered_chunks() {
        let root = std::env::temp_dir().join(format!(
            "coquerythmo-audio-transfer-invalid-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let source = root.join("take.flac");
        fs::write(&source, b"not really flac but framed bytes").unwrap();
        let metadata =
            AudioTransferMetadata::from_file("take_2", &source, target(), recorded()).unwrap();
        let mut receiver = AudioTransferReceiver::default();
        receiver
            .begin(metadata.clone(), &root.join("received"))
            .unwrap();
        assert!(receiver
            .push_base64(&metadata.transfer_id, 1, &STANDARD.encode(b"bad"))
            .is_err());
        receiver
            .push_base64(&metadata.transfer_id, 0, &STANDARD.encode(b"bad"))
            .unwrap();
        assert!(receiver.finish(&metadata.transfer_id).is_err());
        let _ = fs::remove_dir_all(root);
    }
}
