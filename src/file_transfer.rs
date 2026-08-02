//! Bounded, integrity-checked file transfers shared by online assets.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};

use crate::integrity::Sha1;

pub const FILE_CHUNK_BYTES: usize = 192 * 1024;
pub const MAX_PROJECT_BYTES: u64 = 64 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileTransferMetadata {
    pub transfer_id: String,
    pub file_name: String,
    pub total_bytes: u64,
    pub total_chunks: usize,
    pub chunk_size: usize,
    pub sha1: String,
}

impl FileTransferMetadata {
    pub fn from_path(transfer_id: impl Into<String>, path: &Path) -> Result<Self, String> {
        let transfer_id = transfer_id.into();
        validate_transfer_id(&transfer_id)?;
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| "project path has no portable file name".to_string())?
            .to_owned();
        validate_project_file_name(&file_name)?;
        let mut file = File::open(path).map_err(|error| format!("cannot open project: {error}"))?;
        let mut digest = Sha1::new();
        let mut total_bytes = 0_u64;
        let mut buffer = vec![0_u8; FILE_CHUNK_BYTES];
        loop {
            let read = file
                .read(&mut buffer)
                .map_err(|error| format!("cannot hash project: {error}"))?;
            if read == 0 {
                break;
            }
            digest.update(&buffer[..read]);
            total_bytes = total_bytes.saturating_add(read as u64);
            if total_bytes > MAX_PROJECT_BYTES {
                return Err("project transfer exceeds the supported size".into());
            }
        }
        if total_bytes == 0 {
            return Err("project transfer cannot be empty".into());
        }
        let total_chunks = total_bytes.div_ceil(FILE_CHUNK_BYTES as u64) as usize;
        let metadata = Self {
            transfer_id,
            file_name,
            total_bytes,
            total_chunks,
            chunk_size: FILE_CHUNK_BYTES,
            sha1: digest.finalize_hex(),
        };
        metadata.validate()?;
        Ok(metadata)
    }

    pub fn validate(&self) -> Result<(), String> {
        validate_transfer_id(&self.transfer_id)?;
        validate_project_file_name(&self.file_name)?;
        if self.total_bytes == 0 || self.total_bytes > MAX_PROJECT_BYTES {
            return Err("project transfer size is invalid".into());
        }
        if self.chunk_size == 0 || self.chunk_size > FILE_CHUNK_BYTES {
            return Err("project transfer chunk size is invalid".into());
        }
        let expected = self.total_bytes.div_ceil(self.chunk_size as u64) as usize;
        if self.total_chunks != expected {
            return Err("project transfer chunk count is invalid".into());
        }
        if self.sha1.len() != 40
            || !self
                .sha1
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err("project transfer SHA-1 is invalid".into());
        }
        Ok(())
    }
}

pub struct FileChunkReader {
    file: File,
    metadata: FileTransferMetadata,
    index: usize,
}

impl FileChunkReader {
    pub fn open(path: &Path, metadata: &FileTransferMetadata) -> Result<Self, String> {
        metadata.validate()?;
        Ok(Self {
            file: File::open(path)
                .map_err(|error| format!("cannot open transfer file: {error}"))?,
            metadata: metadata.clone(),
            index: 0,
        })
    }
}

impl Iterator for FileChunkReader {
    type Item = Result<(usize, String), String>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.metadata.total_chunks {
            return None;
        }
        let mut bytes = vec![0_u8; self.metadata.chunk_size];
        let read = match self.file.read(&mut bytes) {
            Ok(read) if read > 0 => read,
            Ok(_) => return Some(Err("transfer file ended early".into())),
            Err(error) => return Some(Err(format!("cannot read transfer chunk: {error}"))),
        };
        bytes.truncate(read);
        let chunk = (self.index, STANDARD.encode(bytes));
        self.index += 1;
        Some(Ok(chunk))
    }
}

pub struct ReceivedFile {
    pub metadata: FileTransferMetadata,
    pub path: PathBuf,
}

struct ActiveFile {
    metadata: FileTransferMetadata,
    temporary_path: PathBuf,
    final_path: PathBuf,
    file: File,
    digest: Sha1,
    next_index: usize,
    received_bytes: u64,
}

#[derive(Default)]
pub struct FileTransferReceiver {
    active: Option<ActiveFile>,
}

impl FileTransferReceiver {
    pub fn is_active(&self) -> bool {
        self.active.is_some()
    }

    pub fn begin(
        &mut self,
        metadata: FileTransferMetadata,
        destination: &Path,
    ) -> Result<(), String> {
        metadata.validate()?;
        if self.active.is_some() {
            return Err("another project transfer is active".into());
        }
        fs::create_dir_all(destination)
            .map_err(|error| format!("cannot create transfer directory: {error}"))?;
        let (final_path, temporary_path) = available_paths(destination, &metadata.file_name);
        let file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary_path)
            .map_err(|error| format!("cannot create project transfer file: {error}"))?;
        self.active = Some(ActiveFile {
            metadata,
            temporary_path,
            final_path,
            file,
            digest: Sha1::new(),
            next_index: 0,
            received_bytes: 0,
        });
        Ok(())
    }

    pub fn push_base64(&mut self, index: usize, data: &str) -> Result<f32, String> {
        let transfer = self
            .active
            .as_mut()
            .ok_or_else(|| "no project transfer is active".to_string())?;
        if index != transfer.next_index {
            return Err(format!(
                "out-of-order project chunk: expected {}, received {index}",
                transfer.next_index
            ));
        }
        let bytes = STANDARD
            .decode(data)
            .map_err(|error| format!("invalid project chunk: {error}"))?;
        if bytes.is_empty() || bytes.len() > transfer.metadata.chunk_size {
            return Err("project chunk size is invalid".into());
        }
        let received = transfer.received_bytes.saturating_add(bytes.len() as u64);
        if received > transfer.metadata.total_bytes {
            return Err("project transfer exceeds its announced size".into());
        }
        if STANDARD.encode(&bytes) != data {
            return Err("project chunk is not canonical base64".into());
        }
        transfer
            .file
            .write_all(&bytes)
            .map_err(|error| format!("cannot write project chunk: {error}"))?;
        transfer.digest.update(&bytes);
        transfer.received_bytes = received;
        transfer.next_index += 1;
        Ok(received as f32 / transfer.metadata.total_bytes as f32)
    }

    pub fn finish(&mut self, transfer_id: &str) -> Result<ReceivedFile, String> {
        let mut transfer = self
            .active
            .take()
            .ok_or_else(|| "no project transfer is active".to_string())?;
        let result = (|| {
            if transfer.metadata.transfer_id != transfer_id
                || transfer.next_index != transfer.metadata.total_chunks
                || transfer.received_bytes != transfer.metadata.total_bytes
            {
                return Err("project transfer ended before completion".into());
            }
            transfer
                .file
                .flush()
                .map_err(|error| format!("cannot flush project transfer: {error}"))?;
            if transfer.digest.clone().finalize_hex() != transfer.metadata.sha1 {
                return Err("project transfer SHA-1 mismatch".into());
            }
            drop(transfer.file);
            fs::rename(&transfer.temporary_path, &transfer.final_path)
                .map_err(|error| format!("cannot finalize project transfer: {error}"))?;
            Ok(ReceivedFile {
                metadata: transfer.metadata,
                path: transfer.final_path,
            })
        })();
        if result.is_err() {
            let _ = fs::remove_file(&transfer.temporary_path);
        }
        result
    }

    pub fn cancel(&mut self) {
        if let Some(transfer) = self.active.take() {
            drop(transfer.file);
            let _ = fs::remove_file(transfer.temporary_path);
        }
    }
}

impl Drop for FileTransferReceiver {
    fn drop(&mut self) {
        self.cancel();
    }
}

fn available_paths(destination: &Path, file_name: &str) -> (PathBuf, PathBuf) {
    let path = Path::new(file_name);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("project");
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("coquerythmo");
    for suffix in 0..10_000_u32 {
        let name = if suffix == 0 {
            file_name.to_owned()
        } else {
            format!("{stem} ({suffix}).{extension}")
        };
        let final_path = destination.join(&name);
        let temporary_path = destination.join(format!(".{name}.part"));
        if !final_path.exists() && !temporary_path.exists() {
            return (final_path, temporary_path);
        }
    }
    // The bounded loop above prevents an attacker from forcing an unbounded allocation.
    (
        destination.join(format!("{stem}-received.{extension}")),
        destination.join(format!(".{stem}-received.{extension}.part")),
    )
}

fn validate_transfer_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id.len() > 96
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err("invalid project transfer id".into());
    }
    Ok(())
}

fn validate_project_file_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.len() > 256
        || name.trim() != name
        || name == "."
        || name == ".."
        || name.chars().any(char::is_control)
        || name
            .chars()
            .any(|character| matches!(character, '/' | '\\' | ':'))
        || !name.to_ascii_lowercase().ends_with(".coquerythmo")
    {
        return Err("project file name is not portable".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunked_project_round_trip_is_atomic_and_unique() {
        let root = std::env::temp_dir().join(format!(
            "coquerythmo-project-transfer-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let source = root.join("scene.coquerythmo");
        fs::write(&source, vec![0x5a; FILE_CHUNK_BYTES + 17]).unwrap();
        let metadata = FileTransferMetadata::from_path("transfer_1", &source).unwrap();
        let mut receiver = FileTransferReceiver::default();
        let destination = root.join("transferred_projects");
        receiver.begin(metadata.clone(), &destination).unwrap();
        for chunk in FileChunkReader::open(&source, &metadata).unwrap() {
            let (index, data) = chunk.unwrap();
            receiver.push_base64(index, &data).unwrap();
        }
        let received = receiver.finish(&metadata.transfer_id).unwrap();
        assert_eq!(fs::read(received.path).unwrap(), fs::read(source).unwrap());
        assert!(!destination.join(".scene.coquerythmo.part").exists());
        fs::write(destination.join("scene.coquerythmo"), b"existing").unwrap();
        let mut second = FileTransferReceiver::default();
        second.begin(metadata, &destination).unwrap();
        assert!(destination.join(".scene (1).coquerythmo.part").exists());
        second.cancel();
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn checksum_failure_removes_the_partial_file() {
        let root = std::env::temp_dir().join(format!(
            "coquerythmo-project-transfer-sha1-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let source = root.join("scene.coquerythmo");
        fs::write(&source, b"project").unwrap();
        let mut metadata = FileTransferMetadata::from_path("transfer_sha1", &source).unwrap();
        metadata.sha1 = "0".repeat(40);
        let destination = root.join("transferred_projects");
        let mut receiver = FileTransferReceiver::default();
        receiver.begin(metadata.clone(), &destination).unwrap();
        let data = STANDARD.encode(b"project");
        receiver.push_base64(0, &data).unwrap();
        assert!(receiver.finish(&metadata.transfer_id).is_err());
        assert!(!destination.join(".scene.coquerythmo.part").exists());
        assert!(!destination.join("scene.coquerythmo").exists());
        let _ = fs::remove_dir_all(root);
    }
}
