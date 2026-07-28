//! Portable `.coquerythmo` project containers and legacy JSON loading.
//!
//! The container is deliberately simple and dependency-free. Its little-endian
//! wire layout is:
//!
//! ```text
//! MAGIC[16], version:u32, entry_count:u32
//! repeated entry_count times:
//!   "ENTR", name_len:u32, payload_len:u64, name[UTF-8], payload, crc32:u32
//! "DONE"
//! ```
//!
//! `manifest.json` is always the first entry. Payloads are copied in bounded
//! chunks and protected by CRC-32, so source videos and other large media never
//! need to be held in memory.

use crate::export::ProjectData;
use crate::integrity::Sha1;
use crate::project::Project;
use crate::project_metadata::{Huuid, TransactionJournal};
use crate::recording::{AudioAssetId, RecordingProject, TransactionLog};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, BufWriter, Read, Seek, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

pub const PROJECT_EXTENSION: &str = "coquerythmo";

const MAGIC: &[u8; 16] = b"COQUERYTHMO\0\r\n\x1a\0";
const ENTRY_MAGIC: &[u8; 4] = b"ENTR";
const FOOTER_MAGIC: &[u8; 4] = b"DONE";
const FORMAT_NAME: &str = "coquerythmo";
const FORMAT_VERSION: u32 = 1;
const MANIFEST_ENTRY: &str = "manifest.json";
const MAX_ENTRY_COUNT: u32 = 4096;
const MAX_ENTRY_NAME_BYTES: usize = 1024;
const MAX_MANIFEST_BYTES: u64 = 64 * 1024 * 1024;
const COPY_BUFFER_BYTES: usize = 1024 * 1024;
const DEFAULT_LANGUAGE_ID: &str = "default";

static UNIQUE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Identifies how a project was represented on disk.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectFileKind {
    Bundle,
    LegacyJson,
}

/// One instrumental audio file to place in a bundle.
///
/// `language_id` is metadata, not an archive path, and may therefore be an
/// arbitrary user-facing or application-generated language identifier.
#[derive(Clone, Copy, Debug)]
pub struct InstrumentalAssetInput<'a> {
    pub language_id: &'a str,
    pub path: &'a Path,
}

/// One FLAC file referenced by a durable recording project.
#[derive(Clone, Copy, Debug)]
pub struct RecordingAssetInput<'a> {
    pub asset_id: AudioAssetId,
    pub path: &'a Path,
}

/// Recording state to persist alongside a rythmo project.
#[derive(Clone, Copy, Debug)]
pub struct RecordingBundleInput<'a> {
    pub project: &'a RecordingProject,
    pub transaction_log: &'a TransactionLog,
    pub assets: &'a [RecordingAssetInput<'a>],
}

/// Recording state restored from a portable bundle. Extracted FLAC paths stay
/// valid for the lifetime of the parent [`LoadedProject`].
pub struct LoadedRecordingProject {
    pub project: RecordingProject,
    pub transaction_log: TransactionLog,
    pub audio_asset_paths: BTreeMap<AudioAssetId, PathBuf>,
}

/// A loaded project plus the filesystem paths required by media decoders.
///
/// Bundle assets are extracted into a private temporary directory. Keep this
/// value alive for as long as a video/audio decoder uses any returned path.
/// Dropping it removes that extraction directory.
pub struct LoadedProject {
    pub kind: ProjectFileKind,
    pub project_data: ProjectData,
    pub huuid: Option<Huuid>,
    pub transaction_journal: Option<TransactionJournal>,
    pub source_video_path: Option<PathBuf>,
    pub proxy_video_path: Option<PathBuf>,
    pub font_asset_path: Option<PathBuf>,
    pub instrumental_audio_paths: BTreeMap<String, PathBuf>,
    pub recording: Option<LoadedRecordingProject>,
    extraction: Option<ExtractionGuard>,
}

impl LoadedProject {
    pub fn extraction_root(&self) -> Option<&Path> {
        self.extraction.as_ref().map(|guard| guard.path.as_path())
    }

    pub fn is_legacy_json(&self) -> bool {
        self.kind == ProjectFileKind::LegacyJson
    }
}

#[derive(Debug)]
pub enum ProjectArchiveError {
    Io(io::Error),
    Json(serde_json::Error),
    InvalidFormat(String),
    UnsupportedVersion(u32),
    UnsafeEntryName(String),
    TooManyEntries(u32),
    EntryTooLarge { name: String, size: u64, limit: u64 },
    DuplicateEntry(String),
    UndeclaredEntry(String),
    MissingEntry(String),
    ChecksumMismatch(String),
    MissingAsset(PathBuf),
    InvalidAsset(PathBuf),
    DestinationIsAsset(PathBuf),
    DuplicateLanguage(String),
    AssetChangedDuringSave(PathBuf),
    InvalidTransactionJournal(String),
    InvalidRecordingProject(String),
    InvalidRecordingTransactionLog(String),
    MissingRecordingAsset(AudioAssetId),
    DuplicateRecordingAsset(AudioAssetId),
    UndeclaredRecordingAsset(AudioAssetId),
    InvalidRecordingChecksum(AudioAssetId),
    RecordingChecksumMismatch(AudioAssetId),
}

impl fmt::Display for ProjectArchiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "I/O error: {error}"),
            Self::Json(error) => write!(f, "JSON error: {error}"),
            Self::InvalidFormat(reason) => write!(f, "invalid project container: {reason}"),
            Self::UnsupportedVersion(version) => {
                write!(f, "unsupported project container version {version}")
            }
            Self::UnsafeEntryName(name) => write!(f, "unsafe archive entry name: {name}"),
            Self::TooManyEntries(count) => write!(f, "too many archive entries: {count}"),
            Self::EntryTooLarge { name, size, limit } => {
                write!(f, "archive entry {name} is too large ({size} > {limit})")
            }
            Self::DuplicateEntry(name) => write!(f, "duplicate archive entry: {name}"),
            Self::UndeclaredEntry(name) => write!(f, "undeclared archive entry: {name}"),
            Self::MissingEntry(name) => write!(f, "missing archive entry: {name}"),
            Self::ChecksumMismatch(name) => {
                write!(f, "checksum mismatch for archive entry: {name}")
            }
            Self::MissingAsset(path) => {
                write!(f, "project asset does not exist: {}", path.display())
            }
            Self::InvalidAsset(path) => {
                write!(f, "project asset is not a regular file: {}", path.display())
            }
            Self::DestinationIsAsset(path) => write!(
                f,
                "project destination would overwrite an embedded asset: {}",
                path.display()
            ),
            Self::DuplicateLanguage(language) => {
                write!(f, "duplicate instrumental language identifier: {language}")
            }
            Self::AssetChangedDuringSave(path) => {
                write!(
                    f,
                    "project asset changed while it was being saved: {}",
                    path.display()
                )
            }
            Self::InvalidTransactionJournal(reason) => {
                write!(f, "invalid transaction journal: {reason}")
            }
            Self::InvalidRecordingProject(reason) => {
                write!(f, "invalid recording project: {reason}")
            }
            Self::InvalidRecordingTransactionLog(reason) => {
                write!(f, "invalid recording transaction log: {reason}")
            }
            Self::MissingRecordingAsset(id) => {
                write!(f, "recording asset {id} has no FLAC input")
            }
            Self::DuplicateRecordingAsset(id) => {
                write!(f, "recording asset {id} is declared more than once")
            }
            Self::UndeclaredRecordingAsset(id) => {
                write!(f, "FLAC input targets undeclared recording asset {id}")
            }
            Self::InvalidRecordingChecksum(id) => {
                write!(f, "recording asset {id} does not contain a SHA-1 checksum")
            }
            Self::RecordingChecksumMismatch(id) => {
                write!(f, "recording asset {id} does not match its SHA-1 checksum")
            }
        }
    }
}

impl std::error::Error for ProjectArchiveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for ProjectArchiveError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for ProjectArchiveError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

#[derive(Serialize, Deserialize)]
struct BundleManifest {
    format: String,
    format_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    huuid: Option<Huuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    transactions: Option<TransactionJournal>,
    project: ProjectData,
    assets: BundleAssets,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    recording: Option<BundleRecordingManifest>,
}

/// Metadata created only after a complete bundle has been installed at its
/// destination path.
#[derive(Clone)]
pub struct SavedProjectMetadata {
    pub huuid: Huuid,
    /// Exact portable journal stored in the bundle. Asset-path sanitization can
    /// change checkpoint hashes, so callers should adopt this snapshot after a
    /// successful save when they synchronize journal prefixes with peers.
    pub transaction_journal: Option<TransactionJournal>,
}

#[derive(Serialize, Deserialize)]
struct BundleAssets {
    source_video: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    proxy_video: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    font: Option<String>,
    #[serde(default)]
    instrumentals: Vec<InstrumentalAssetManifest>,
}

#[derive(Serialize, Deserialize)]
struct InstrumentalAssetManifest {
    language_id: String,
    entry: String,
}

#[derive(Serialize, Deserialize)]
struct BundleRecordingManifest {
    project: RecordingProject,
    transaction_log: TransactionLog,
    #[serde(default)]
    assets: Vec<RecordingAssetManifest>,
}

#[derive(Serialize, Deserialize)]
struct RecordingAssetManifest {
    asset_id: AudioAssetId,
    entry: String,
}

struct FileEntryToWrite {
    name: String,
    path: PathBuf,
    len: u64,
    expected_recording_sha1: Option<(AudioAssetId, [u8; 20])>,
}

struct ExtractionGuard {
    path: PathBuf,
}

impl Drop for ExtractionGuard {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.path) {
            if error.kind() != io::ErrorKind::NotFound {
                log::warn!(
                    "Could not remove extracted project assets at {}: {error}",
                    self.path.display()
                );
            }
        }
    }
}

/// Save a complete portable project, including every language-specific
/// instrumental exposed by [`Project::language_snapshots`].
pub fn save_bundle(
    project: &Project,
    fps: f64,
    bundle_path: &Path,
    source_video: &Path,
    proxy_video: Option<&Path>,
    font_asset: Option<&Path>,
) -> Result<(), ProjectArchiveError> {
    save_bundle_with_metadata(
        project,
        fps,
        bundle_path,
        source_video,
        proxy_video,
        font_asset,
        None,
    )
    .map(|_| ())
}

/// Save a complete bundle and return the HUUID assigned to this successful
/// save. The journal is snapshotted into the manifest when supplied.
pub fn save_bundle_with_metadata(
    project: &Project,
    fps: f64,
    bundle_path: &Path,
    source_video: &Path,
    proxy_video: Option<&Path>,
    font_asset: Option<&Path>,
    transaction_journal: Option<&TransactionJournal>,
) -> Result<SavedProjectMetadata, ProjectArchiveError> {
    save_bundle_with_recording_data(
        project,
        fps,
        bundle_path,
        source_video,
        proxy_video,
        font_asset,
        transaction_journal,
        None,
    )
}

/// Save all current project metadata, optionally including the recording
/// timeline, its transaction log, and every referenced FLAC file.
pub fn save_bundle_with_recording_data(
    project: &Project,
    fps: f64,
    bundle_path: &Path,
    source_video: &Path,
    proxy_video: Option<&Path>,
    font_asset: Option<&Path>,
    transaction_journal: Option<&TransactionJournal>,
    recording: Option<RecordingBundleInput<'_>>,
) -> Result<SavedProjectMetadata, ProjectArchiveError> {
    let instrumental_paths: Vec<(String, PathBuf)> = project
        .language_snapshots()
        .into_iter()
        .filter_map(|snapshot| {
            snapshot
                .project
                .settings()
                .instrumental_audio_path
                .as_deref()
                .filter(|path| !path.trim().is_empty())
                .map(|path| (snapshot.language.id.to_string(), PathBuf::from(path)))
        })
        .collect();
    let instrumentals: Vec<InstrumentalAssetInput<'_>> = instrumental_paths
        .iter()
        .map(|(language_id, path)| InstrumentalAssetInput { language_id, path })
        .collect();

    save_bundle_with_instrumentals_and_recording_data(
        project,
        fps,
        bundle_path,
        source_video,
        &instrumentals,
        proxy_video,
        font_asset,
        transaction_journal,
        recording,
    )
}

/// Save a complete portable project with every language-specific instrumental.
pub fn save_bundle_with_instrumentals(
    project: &Project,
    fps: f64,
    bundle_path: &Path,
    source_video: &Path,
    instrumentals: &[InstrumentalAssetInput<'_>],
    proxy_video: Option<&Path>,
    font_asset: Option<&Path>,
) -> Result<(), ProjectArchiveError> {
    save_bundle_with_instrumentals_and_metadata(
        project,
        fps,
        bundle_path,
        source_video,
        instrumentals,
        proxy_video,
        font_asset,
        None,
    )
    .map(|_| ())
}

pub fn save_bundle_with_instrumentals_and_metadata(
    project: &Project,
    fps: f64,
    bundle_path: &Path,
    source_video: &Path,
    instrumentals: &[InstrumentalAssetInput<'_>],
    proxy_video: Option<&Path>,
    font_asset: Option<&Path>,
    transaction_journal: Option<&TransactionJournal>,
) -> Result<SavedProjectMetadata, ProjectArchiveError> {
    save_bundle_with_instrumentals_and_recording_data(
        project,
        fps,
        bundle_path,
        source_video,
        instrumentals,
        proxy_video,
        font_asset,
        transaction_journal,
        None,
    )
}

pub fn save_bundle_with_instrumentals_and_recording_data(
    project: &Project,
    fps: f64,
    bundle_path: &Path,
    source_video: &Path,
    instrumentals: &[InstrumentalAssetInput<'_>],
    proxy_video: Option<&Path>,
    font_asset: Option<&Path>,
    transaction_journal: Option<&TransactionJournal>,
    recording: Option<RecordingBundleInput<'_>>,
) -> Result<SavedProjectMetadata, ProjectArchiveError> {
    let transaction_journal = if let Some(journal) = transaction_journal {
        journal
            .validate_integrity()
            .map_err(|error| ProjectArchiveError::InvalidTransactionJournal(error.to_string()))?;
        if !crate::project_metadata::fps_matches(journal.checkpoint().source_fps, fps) {
            // A fresh session may have created its journal before the video
            // FPS was known. The project snapshot is authoritative at save.
            Some(
                crate::project_metadata::TransactionJournal::from_project(project, fps).map_err(
                    |error| ProjectArchiveError::InvalidTransactionJournal(error.to_string()),
                )?,
            )
        } else {
            Some(journal.clone())
        }
    } else {
        None
    };
    ensure_destination_is_not_asset(bundle_path, source_video)?;
    if let Some(path) = proxy_video {
        ensure_destination_is_not_asset(bundle_path, path)?;
    }
    if let Some(path) = font_asset {
        ensure_destination_is_not_asset(bundle_path, path)?;
    }
    for instrumental in instrumentals {
        ensure_destination_is_not_asset(bundle_path, instrumental.path)?;
    }

    let source_entry = entry_name_for_asset("media/source", source_video);
    let source = file_entry(source_entry.clone(), source_video)?;
    let proxy = proxy_video
        .map(|path| file_entry(entry_name_for_asset("media/proxy", path), path))
        .transpose()?;
    let font = font_asset
        .map(|path| file_entry(entry_name_for_asset("fonts/rythmo", path), path))
        .transpose()?;

    let mut seen_languages = HashSet::new();
    let mut instrumental_entries = Vec::with_capacity(instrumentals.len());
    let mut instrumental_manifest = Vec::with_capacity(instrumentals.len());
    for (index, instrumental) in instrumentals.iter().enumerate() {
        let language_id = instrumental.language_id.trim();
        if language_id.is_empty() || language_id.chars().any(char::is_control) {
            return Err(ProjectArchiveError::InvalidFormat(
                "instrumental language identifier is empty or contains control characters".into(),
            ));
        }
        if !seen_languages.insert(language_id.to_string()) {
            return Err(ProjectArchiveError::DuplicateLanguage(
                language_id.to_string(),
            ));
        }
        let stem = format!("audio/instrumental-{index:04}");
        let entry_name = entry_name_for_asset(&stem, instrumental.path);
        instrumental_entries.push(file_entry(entry_name.clone(), instrumental.path)?);
        instrumental_manifest.push(InstrumentalAssetManifest {
            language_id: language_id.to_string(),
            entry: entry_name,
        });
    }

    let prepared_recording = recording
        .map(|recording| prepare_recording_bundle(recording, bundle_path, fps))
        .transpose()?;
    let (recording_manifest, recording_entries) = match prepared_recording {
        Some((manifest, entries)) => (Some(manifest), entries),
        None => (None, Vec::new()),
    };

    let mut project_data = ProjectData::from_project(project, fps);
    // Never leave a machine-specific path in a portable manifest.
    rewrite_project_instrumental_paths_for_bundle(&mut project_data, &instrumental_manifest);
    let mut stored_journal = transaction_journal;
    if let Some(journal) = &mut stored_journal {
        journal
            .rewrite_checkpoint(|checkpoint| {
                rewrite_project_instrumental_paths_for_bundle(checkpoint, &instrumental_manifest)
            })
            .map_err(|error| ProjectArchiveError::InvalidTransactionJournal(error.to_string()))?;
    }

    let huuid = Huuid::generate();
    let saved_transaction_journal = stored_journal.clone();
    let manifest = BundleManifest {
        format: FORMAT_NAME.to_string(),
        format_version: FORMAT_VERSION,
        huuid: Some(huuid.clone()),
        transactions: stored_journal,
        project: project_data,
        assets: BundleAssets {
            source_video: source_entry,
            proxy_video: proxy.as_ref().map(|entry| entry.name.clone()),
            font: font.as_ref().map(|entry| entry.name.clone()),
            instrumentals: instrumental_manifest,
        },
        recording: recording_manifest,
    };
    let manifest_bytes = serde_json::to_vec(&manifest)?;
    if manifest_bytes.len() as u64 > MAX_MANIFEST_BYTES {
        return Err(ProjectArchiveError::EntryTooLarge {
            name: MANIFEST_ENTRY.to_string(),
            size: manifest_bytes.len() as u64,
            limit: MAX_MANIFEST_BYTES,
        });
    }

    let mut entries = Vec::with_capacity(1 + instrumentals.len() + recording_entries.len() + 3);
    entries.push(source);
    if let Some(proxy) = proxy {
        entries.push(proxy);
    }
    if let Some(font) = font {
        entries.push(font);
    }
    entries.extend(instrumental_entries);
    entries.extend(recording_entries);

    let entry_count = u32::try_from(entries.len() + 1)
        .map_err(|_| ProjectArchiveError::TooManyEntries(u32::MAX))?;
    if entry_count > MAX_ENTRY_COUNT {
        return Err(ProjectArchiveError::TooManyEntries(entry_count));
    }

    let (temporary_path, temporary_file) = create_temporary_file_near(bundle_path)?;
    let write_result = (|| -> Result<(), ProjectArchiveError> {
        let mut writer = BufWriter::with_capacity(COPY_BUFFER_BYTES, temporary_file);
        writer.write_all(MAGIC)?;
        write_u32(&mut writer, FORMAT_VERSION)?;
        write_u32(&mut writer, entry_count)?;
        write_bytes_entry(&mut writer, MANIFEST_ENTRY, &manifest_bytes)?;
        for entry in entries {
            write_file_entry(&mut writer, &entry)?;
        }
        writer.write_all(FOOTER_MAGIC)?;
        writer.flush()?;
        let file = writer
            .into_inner()
            .map_err(|error| ProjectArchiveError::Io(error.into_error()))?;
        file.sync_all()?;
        Ok(())
    })();

    if let Err(error) = write_result {
        let _ = fs::remove_file(&temporary_path);
        return Err(error);
    }
    if let Err(error) = commit_temporary_file(&temporary_path, bundle_path) {
        let _ = fs::remove_file(&temporary_path);
        return Err(ProjectArchiveError::Io(error));
    }
    Ok(SavedProjectMetadata {
        huuid,
        transaction_journal: saved_transaction_journal,
    })
}

/// Load either a `.coquerythmo` bundle or a legacy `ProjectData` JSON file.
pub fn load_project_file(path: &Path) -> Result<LoadedProject, ProjectArchiveError> {
    let mut file = File::open(path)?;
    let mut prefix = [0_u8; MAGIC.len()];
    let is_bundle = if file.metadata()?.len() >= MAGIC.len() as u64 {
        file.read_exact(&mut prefix)?;
        &prefix == MAGIC
    } else {
        false
    };
    file.rewind()?;
    if is_bundle {
        load_bundle(file)
    } else {
        load_legacy_json(file)
    }
}

fn load_legacy_json(file: File) -> Result<LoadedProject, ProjectArchiveError> {
    let reader = BufReader::new(file);
    let project_data: ProjectData = serde_json::from_reader(reader)?;
    project_data
        .validate_line_ids()
        .map_err(ProjectArchiveError::InvalidFormat)?;
    let mut instrumental_audio_paths = BTreeMap::new();
    if let Some(path) = project_data
        .settings
        .instrumental_audio_path
        .as_deref()
        .filter(|path| !path.trim().is_empty())
    {
        let language_id = project_data
            .active_language_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| DEFAULT_LANGUAGE_ID.to_string());
        instrumental_audio_paths.insert(language_id, PathBuf::from(path));
    }
    for language in &project_data.languages {
        if let Some(path) = language
            .project
            .settings
            .instrumental_audio_path
            .as_deref()
            .filter(|path| !path.trim().is_empty())
        {
            instrumental_audio_paths.insert(language.id.to_string(), PathBuf::from(path));
        }
    }
    Ok(LoadedProject {
        kind: ProjectFileKind::LegacyJson,
        project_data,
        huuid: None,
        transaction_journal: None,
        source_video_path: None,
        proxy_video_path: None,
        font_asset_path: None,
        instrumental_audio_paths,
        recording: None,
        extraction: None,
    })
}

fn load_bundle(file: File) -> Result<LoadedProject, ProjectArchiveError> {
    let file_len = file.metadata()?.len();
    let mut reader = BufReader::with_capacity(COPY_BUFFER_BYTES, file);
    let mut magic = [0_u8; MAGIC.len()];
    reader.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(ProjectArchiveError::InvalidFormat("bad magic".into()));
    }
    let version = read_u32(&mut reader)?;
    if version != FORMAT_VERSION {
        return Err(ProjectArchiveError::UnsupportedVersion(version));
    }
    let entry_count = read_u32(&mut reader)?;
    if entry_count == 0 {
        return Err(ProjectArchiveError::InvalidFormat(
            "container has no manifest entry".into(),
        ));
    }
    if entry_count > MAX_ENTRY_COUNT {
        return Err(ProjectArchiveError::TooManyEntries(entry_count));
    }

    let first_header = read_entry_header(&mut reader, file_len)?;
    if first_header.name != MANIFEST_ENTRY {
        return Err(ProjectArchiveError::InvalidFormat(
            "manifest.json must be the first entry".into(),
        ));
    }
    if first_header.len > MAX_MANIFEST_BYTES {
        return Err(ProjectArchiveError::EntryTooLarge {
            name: first_header.name,
            size: first_header.len,
            limit: MAX_MANIFEST_BYTES,
        });
    }
    let manifest_bytes = read_entry_to_vec(&mut reader, &first_header)?;
    let mut manifest: BundleManifest = serde_json::from_slice(&manifest_bytes)?;
    validate_manifest(&manifest, version, entry_count)?;
    manifest
        .project
        .validate_line_ids()
        .map_err(ProjectArchiveError::InvalidFormat)?;
    if let Some(journal) = &manifest.transactions {
        journal
            .validate_integrity()
            .map_err(|error| ProjectArchiveError::InvalidTransactionJournal(error.to_string()))?;
        if !crate::project_metadata::fps_matches(
            journal.checkpoint().source_fps,
            manifest.project.source_fps,
        ) {
            return Err(ProjectArchiveError::InvalidTransactionJournal(
                "checkpoint FPS does not match the project manifest".into(),
            ));
        }
    }
    if let Some(recording) = &manifest.recording {
        if !crate::project_metadata::fps_matches(
            recording.project.timeline_fps(),
            manifest.project.source_fps,
        ) {
            return Err(ProjectArchiveError::InvalidRecordingProject(
                "timeline FPS does not match the project manifest".into(),
            ));
        }
        validate_recording_archive_state(&recording.project, &recording.transaction_log)?;
    }

    let extraction = create_extraction_guard()?;
    let expected_names = manifest_asset_names(&manifest.assets, manifest.recording.as_ref())?;
    let mut remaining = expected_names;
    let mut extracted_paths = HashMap::new();
    let mut seen_entries = HashSet::new();
    seen_entries.insert(MANIFEST_ENTRY.to_string());

    for _ in 1..entry_count {
        let header = read_entry_header(&mut reader, file_len)?;
        if !seen_entries.insert(header.name.clone()) {
            return Err(ProjectArchiveError::DuplicateEntry(header.name));
        }
        if !remaining.remove(&header.name) {
            return Err(ProjectArchiveError::UndeclaredEntry(header.name));
        }
        let output_path = extraction_path(&extraction.path, &header.name)?;
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&output_path)?;
        read_entry_to_file(&mut reader, &header, output)?;
        extracted_paths.insert(header.name, output_path);
    }

    if let Some(name) = remaining.into_iter().next() {
        return Err(ProjectArchiveError::MissingEntry(name));
    }
    let mut footer = [0_u8; FOOTER_MAGIC.len()];
    reader.read_exact(&mut footer)?;
    if &footer != FOOTER_MAGIC {
        return Err(ProjectArchiveError::InvalidFormat("bad footer".into()));
    }
    let mut trailing = [0_u8; 1];
    if reader.read(&mut trailing)? != 0 {
        return Err(ProjectArchiveError::InvalidFormat(
            "trailing bytes after container footer".into(),
        ));
    }

    let source_video_path = extracted_paths
        .get(&manifest.assets.source_video)
        .cloned()
        .ok_or_else(|| ProjectArchiveError::MissingEntry(manifest.assets.source_video.clone()))?;
    let proxy_video_path = manifest
        .assets
        .proxy_video
        .as_ref()
        .map(|entry| {
            extracted_paths
                .get(entry)
                .cloned()
                .ok_or_else(|| ProjectArchiveError::MissingEntry(entry.clone()))
        })
        .transpose()?;
    let font_asset_path = manifest
        .assets
        .font
        .as_ref()
        .map(|entry| {
            extracted_paths
                .get(entry)
                .cloned()
                .ok_or_else(|| ProjectArchiveError::MissingEntry(entry.clone()))
        })
        .transpose()?;

    let mut instrumental_audio_paths = BTreeMap::new();
    for instrumental in &manifest.assets.instrumentals {
        let path = extracted_paths
            .get(&instrumental.entry)
            .cloned()
            .ok_or_else(|| ProjectArchiveError::MissingEntry(instrumental.entry.clone()))?;
        instrumental_audio_paths.insert(instrumental.language_id.clone(), path);
    }
    reinject_project_instrumental_paths(&mut manifest.project, &instrumental_audio_paths);

    let recording = manifest
        .recording
        .take()
        .map(|recording| {
            let mut audio_asset_paths = BTreeMap::new();
            for asset_manifest in &recording.assets {
                let path = extracted_paths
                    .get(&asset_manifest.entry)
                    .cloned()
                    .ok_or_else(|| {
                        ProjectArchiveError::MissingEntry(asset_manifest.entry.clone())
                    })?;
                let expected_checksum = &recording
                    .project
                    .asset(asset_manifest.asset_id)
                    .ok_or(ProjectArchiveError::UndeclaredRecordingAsset(
                        asset_manifest.asset_id,
                    ))?
                    .checksum;
                verify_recording_file_checksum(asset_manifest.asset_id, &path, expected_checksum)?;
                audio_asset_paths.insert(asset_manifest.asset_id, path);
            }
            Ok::<_, ProjectArchiveError>(LoadedRecordingProject {
                project: recording.project,
                transaction_log: recording.transaction_log,
                audio_asset_paths,
            })
        })
        .transpose()?;

    Ok(LoadedProject {
        kind: ProjectFileKind::Bundle,
        project_data: manifest.project,
        huuid: manifest.huuid,
        transaction_journal: manifest.transactions,
        source_video_path: Some(source_video_path),
        proxy_video_path,
        font_asset_path,
        instrumental_audio_paths,
        recording,
        extraction: Some(extraction),
    })
}

fn rewrite_project_instrumental_paths_for_bundle(
    project: &mut ProjectData,
    instrumentals: &[InstrumentalAssetManifest],
) {
    let entries: HashMap<&str, &str> = instrumentals
        .iter()
        .map(|asset| (asset.language_id.as_str(), asset.entry.as_str()))
        .collect();
    project.settings.instrumental_audio_path = project
        .active_language_id
        .and_then(|id| entries.get(id.to_string().as_str()).copied())
        .or_else(|| entries.get(DEFAULT_LANGUAGE_ID).copied())
        .or_else(|| {
            (entries.len() == 1)
                .then(|| entries.values().next().copied())
                .flatten()
        })
        .map(str::to_string);

    for language in &mut project.languages {
        language.project.settings.instrumental_audio_path = entries
            .get(language.id.to_string().as_str())
            .copied()
            .map(str::to_string);
    }
}

fn reinject_project_instrumental_paths(
    project: &mut ProjectData,
    instrumentals: &BTreeMap<String, PathBuf>,
) {
    project.settings.instrumental_audio_path = project
        .active_language_id
        .and_then(|id| instrumentals.get(&id.to_string()))
        .or_else(|| instrumentals.get(DEFAULT_LANGUAGE_ID))
        .or_else(|| {
            (instrumentals.len() == 1)
                .then(|| instrumentals.values().next())
                .flatten()
        })
        .map(|path| path.to_string_lossy().into_owned());

    for language in &mut project.languages {
        language.project.settings.instrumental_audio_path = instrumentals
            .get(&language.id.to_string())
            .map(|path| path.to_string_lossy().into_owned());
    }
}

fn validate_manifest(
    manifest: &BundleManifest,
    header_version: u32,
    entry_count: u32,
) -> Result<(), ProjectArchiveError> {
    if manifest.format != FORMAT_NAME {
        return Err(ProjectArchiveError::InvalidFormat(format!(
            "unexpected manifest format {:?}",
            manifest.format
        )));
    }
    if manifest.format_version != header_version {
        return Err(ProjectArchiveError::InvalidFormat(
            "header and manifest versions differ".into(),
        ));
    }
    let names = manifest_asset_names(&manifest.assets, manifest.recording.as_ref())?;
    let expected_count = u32::try_from(names.len() + 1)
        .map_err(|_| ProjectArchiveError::TooManyEntries(u32::MAX))?;
    if expected_count != entry_count {
        return Err(ProjectArchiveError::InvalidFormat(format!(
            "entry count mismatch: header declares {entry_count}, manifest declares {expected_count}"
        )));
    }
    Ok(())
}

fn manifest_asset_names(
    assets: &BundleAssets,
    recording: Option<&BundleRecordingManifest>,
) -> Result<HashSet<String>, ProjectArchiveError> {
    let mut names = HashSet::new();
    insert_manifest_name(&mut names, &assets.source_video)?;
    if let Some(name) = &assets.proxy_video {
        insert_manifest_name(&mut names, name)?;
    }
    if let Some(name) = &assets.font {
        insert_manifest_name(&mut names, name)?;
    }
    let mut languages = HashSet::new();
    for instrumental in &assets.instrumentals {
        if instrumental.language_id.trim().is_empty()
            || instrumental.language_id.chars().any(char::is_control)
        {
            return Err(ProjectArchiveError::InvalidFormat(
                "invalid instrumental language identifier".into(),
            ));
        }
        if !languages.insert(instrumental.language_id.clone()) {
            return Err(ProjectArchiveError::DuplicateLanguage(
                instrumental.language_id.clone(),
            ));
        }
        insert_manifest_name(&mut names, &instrumental.entry)?;
    }
    if let Some(recording) = recording {
        let mut asset_ids = HashSet::new();
        for asset in &recording.assets {
            if !asset_ids.insert(asset.asset_id) {
                return Err(ProjectArchiveError::DuplicateRecordingAsset(asset.asset_id));
            }
            let project_asset = recording.project.asset(asset.asset_id).ok_or(
                ProjectArchiveError::UndeclaredRecordingAsset(asset.asset_id),
            )?;
            parse_recording_sha1(asset.asset_id, &project_asset.checksum)?;
            insert_manifest_name(&mut names, &asset.entry)?;
        }
        for asset in recording.project.assets() {
            if !asset_ids.contains(&asset.id) {
                return Err(ProjectArchiveError::MissingRecordingAsset(asset.id));
            }
        }
    }
    Ok(names)
}

fn insert_manifest_name(
    names: &mut HashSet<String>,
    name: &str,
) -> Result<(), ProjectArchiveError> {
    validate_entry_name(name)?;
    if name == MANIFEST_ENTRY || !names.insert(name.to_string()) {
        return Err(ProjectArchiveError::DuplicateEntry(name.to_string()));
    }
    Ok(())
}

struct EntryHeader {
    name: String,
    len: u64,
}

fn read_entry_header<R: Read + Seek>(
    reader: &mut R,
    file_len: u64,
) -> Result<EntryHeader, ProjectArchiveError> {
    let mut marker = [0_u8; ENTRY_MAGIC.len()];
    reader.read_exact(&mut marker)?;
    if &marker != ENTRY_MAGIC {
        return Err(ProjectArchiveError::InvalidFormat(
            "bad entry marker".into(),
        ));
    }
    let name_len = read_u32(reader)? as usize;
    if name_len == 0 || name_len > MAX_ENTRY_NAME_BYTES {
        return Err(ProjectArchiveError::InvalidFormat(format!(
            "invalid entry name length {name_len}"
        )));
    }
    let payload_len = read_u64(reader)?;
    let mut name_bytes = vec![0_u8; name_len];
    reader.read_exact(&mut name_bytes)?;
    let name = String::from_utf8(name_bytes)
        .map_err(|_| ProjectArchiveError::InvalidFormat("entry name is not UTF-8".into()))?;
    validate_entry_name(&name)?;
    let position = reader.stream_position()?;
    let required = payload_len
        .checked_add(4)
        .ok_or_else(|| ProjectArchiveError::InvalidFormat("entry length overflows".into()))?;
    if required > file_len.saturating_sub(position) {
        return Err(ProjectArchiveError::InvalidFormat(format!(
            "entry {name} exceeds the container length"
        )));
    }
    Ok(EntryHeader {
        name,
        len: payload_len,
    })
}

fn read_entry_to_vec<R: Read>(
    reader: &mut R,
    header: &EntryHeader,
) -> Result<Vec<u8>, ProjectArchiveError> {
    let capacity = usize::try_from(header.len).map_err(|_| ProjectArchiveError::EntryTooLarge {
        name: header.name.clone(),
        size: header.len,
        limit: usize::MAX as u64,
    })?;
    let mut bytes = Vec::with_capacity(capacity);
    copy_payload(reader, &mut bytes, header)?;
    Ok(bytes)
}

fn read_entry_to_file<R: Read>(
    reader: &mut R,
    header: &EntryHeader,
    output: File,
) -> Result<(), ProjectArchiveError> {
    let mut writer = BufWriter::with_capacity(COPY_BUFFER_BYTES, output);
    copy_payload(reader, &mut writer, header)?;
    writer.flush()?;
    let file = writer
        .into_inner()
        .map_err(|error| ProjectArchiveError::Io(error.into_error()))?;
    file.sync_all()?;
    Ok(())
}

fn copy_payload<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
    header: &EntryHeader,
) -> Result<(), ProjectArchiveError> {
    let mut remaining = header.len;
    let mut crc = Crc32::new();
    let mut buffer = vec![0_u8; COPY_BUFFER_BYTES];
    while remaining > 0 {
        let wanted = usize::try_from(remaining.min(buffer.len() as u64)).unwrap_or(buffer.len());
        reader.read_exact(&mut buffer[..wanted])?;
        writer.write_all(&buffer[..wanted])?;
        crc.update(&buffer[..wanted]);
        remaining -= wanted as u64;
    }
    let expected_crc = read_u32(reader)?;
    if crc.finish() != expected_crc {
        return Err(ProjectArchiveError::ChecksumMismatch(header.name.clone()));
    }
    Ok(())
}

fn write_bytes_entry<W: Write>(
    writer: &mut W,
    name: &str,
    payload: &[u8],
) -> Result<(), ProjectArchiveError> {
    write_entry_header(writer, name, payload.len() as u64)?;
    writer.write_all(payload)?;
    let mut crc = Crc32::new();
    crc.update(payload);
    write_u32(writer, crc.finish())?;
    Ok(())
}

fn write_file_entry<W: Write>(
    writer: &mut W,
    entry: &FileEntryToWrite,
) -> Result<(), ProjectArchiveError> {
    write_entry_header(writer, &entry.name, entry.len)?;
    let mut file = File::open(&entry.path)?;
    let mut remaining = entry.len;
    let mut crc = Crc32::new();
    let mut sha1 = entry.expected_recording_sha1.map(|_| Sha1::new());
    let mut buffer = vec![0_u8; COPY_BUFFER_BYTES];
    while remaining > 0 {
        let wanted = usize::try_from(remaining.min(buffer.len() as u64)).unwrap_or(buffer.len());
        let read = file.read(&mut buffer[..wanted])?;
        if read == 0 {
            return Err(ProjectArchiveError::AssetChangedDuringSave(
                entry.path.clone(),
            ));
        }
        writer.write_all(&buffer[..read])?;
        crc.update(&buffer[..read]);
        if let Some(sha1) = &mut sha1 {
            sha1.update(&buffer[..read]);
        }
        remaining -= read as u64;
    }
    let mut extra = [0_u8; 1];
    if file.read(&mut extra)? != 0 {
        return Err(ProjectArchiveError::AssetChangedDuringSave(
            entry.path.clone(),
        ));
    }
    if let (Some((asset_id, expected)), Some(actual)) =
        (entry.expected_recording_sha1, sha1.map(Sha1::finalize))
    {
        if actual != expected {
            return Err(ProjectArchiveError::RecordingChecksumMismatch(asset_id));
        }
    }
    write_u32(writer, crc.finish())?;
    Ok(())
}

fn write_entry_header<W: Write>(
    writer: &mut W,
    name: &str,
    payload_len: u64,
) -> Result<(), ProjectArchiveError> {
    validate_entry_name(name)?;
    let name_len = u32::try_from(name.len())
        .map_err(|_| ProjectArchiveError::UnsafeEntryName(name.to_string()))?;
    writer.write_all(ENTRY_MAGIC)?;
    write_u32(writer, name_len)?;
    write_u64(writer, payload_len)?;
    writer.write_all(name.as_bytes())?;
    Ok(())
}

fn file_entry(name: String, path: &Path) -> Result<FileEntryToWrite, ProjectArchiveError> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(ProjectArchiveError::MissingAsset(path.to_path_buf()));
        }
        Err(error) => return Err(ProjectArchiveError::Io(error)),
    };
    if !metadata.is_file() {
        return Err(ProjectArchiveError::InvalidAsset(path.to_path_buf()));
    }
    Ok(FileEntryToWrite {
        name,
        path: path.to_path_buf(),
        len: metadata.len(),
        expected_recording_sha1: None,
    })
}

fn prepare_recording_bundle(
    recording: RecordingBundleInput<'_>,
    bundle_path: &Path,
    project_fps: f64,
) -> Result<(BundleRecordingManifest, Vec<FileEntryToWrite>), ProjectArchiveError> {
    if !crate::project_metadata::fps_matches(recording.project.timeline_fps(), project_fps) {
        return Err(ProjectArchiveError::InvalidRecordingProject(
            "timeline FPS does not match the saved project FPS".into(),
        ));
    }
    validate_recording_archive_state(recording.project, recording.transaction_log)?;

    let mut seen_assets = HashSet::new();
    let mut manifest_assets = Vec::with_capacity(recording.assets.len());
    let mut file_entries = Vec::with_capacity(recording.assets.len());
    for input in recording.assets {
        if !seen_assets.insert(input.asset_id) {
            return Err(ProjectArchiveError::DuplicateRecordingAsset(input.asset_id));
        }
        let asset = recording.project.asset(input.asset_id).ok_or(
            ProjectArchiveError::UndeclaredRecordingAsset(input.asset_id),
        )?;
        if !input
            .path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("flac"))
        {
            return Err(ProjectArchiveError::InvalidAsset(input.path.to_path_buf()));
        }
        ensure_destination_is_not_asset(bundle_path, input.path)?;
        let expected_sha1 = parse_recording_sha1(input.asset_id, &asset.checksum)?;
        let entry_name = format!(
            "audio/recordings/{:016x}-{}",
            input.asset_id.get(),
            asset.file_name
        );
        validate_entry_name(&entry_name)?;
        let mut entry = file_entry(entry_name.clone(), input.path)?;
        entry.expected_recording_sha1 = Some((input.asset_id, expected_sha1));
        file_entries.push(entry);
        manifest_assets.push(RecordingAssetManifest {
            asset_id: input.asset_id,
            entry: entry_name,
        });
    }

    for asset in recording.project.assets() {
        if !seen_assets.contains(&asset.id) {
            return Err(ProjectArchiveError::MissingRecordingAsset(asset.id));
        }
    }

    Ok((
        BundleRecordingManifest {
            project: recording.project.clone(),
            transaction_log: recording.transaction_log.clone(),
            assets: manifest_assets,
        },
        file_entries,
    ))
}

fn validate_recording_archive_state(
    project: &RecordingProject,
    transaction_log: &TransactionLog,
) -> Result<(), ProjectArchiveError> {
    project
        .validate()
        .map_err(|error| ProjectArchiveError::InvalidRecordingProject(error.to_string()))?;
    transaction_log
        .verify_integrity()
        .map_err(|error| ProjectArchiveError::InvalidRecordingTransactionLog(error.to_string()))?;
    let base = RecordingProject::new(project.timeline_fps())
        .map_err(|error| ProjectArchiveError::InvalidRecordingProject(error.to_string()))?;
    let rebuilt = transaction_log
        .rebuild_from_base(&base)
        .map_err(|error| ProjectArchiveError::InvalidRecordingTransactionLog(error.to_string()))?;
    if &rebuilt != project {
        return Err(ProjectArchiveError::InvalidRecordingTransactionLog(
            "the transaction cursor does not reconstruct the stored recording project".into(),
        ));
    }
    Ok(())
}

fn parse_recording_sha1(
    asset_id: AudioAssetId,
    checksum: &str,
) -> Result<[u8; 20], ProjectArchiveError> {
    if checksum.len() != 40 || !checksum.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ProjectArchiveError::InvalidRecordingChecksum(asset_id));
    }
    let mut digest = [0_u8; 20];
    for (index, output) in digest.iter_mut().enumerate() {
        *output = u8::from_str_radix(&checksum[index * 2..index * 2 + 2], 16)
            .map_err(|_| ProjectArchiveError::InvalidRecordingChecksum(asset_id))?;
    }
    Ok(digest)
}

fn verify_recording_file_checksum(
    asset_id: AudioAssetId,
    path: &Path,
    checksum: &str,
) -> Result<(), ProjectArchiveError> {
    let expected = parse_recording_sha1(asset_id, checksum)?;
    let file = File::open(path)?;
    let mut reader = BufReader::with_capacity(COPY_BUFFER_BYTES, file);
    let mut digest = Sha1::new();
    let mut buffer = vec![0_u8; COPY_BUFFER_BYTES];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    if digest.finalize() != expected {
        return Err(ProjectArchiveError::RecordingChecksumMismatch(asset_id));
    }
    Ok(())
}

fn ensure_destination_is_not_asset(
    destination: &Path,
    asset: &Path,
) -> Result<(), ProjectArchiveError> {
    if comparable_absolute_path(destination) == comparable_absolute_path(asset) {
        return Err(ProjectArchiveError::DestinationIsAsset(asset.to_path_buf()));
    }
    Ok(())
}

fn comparable_absolute_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(path)
        }
    })
}

fn entry_name_for_asset(stem: &str, path: &Path) -> String {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            extension
                .chars()
                .filter(|character| character.is_ascii_alphanumeric())
                .take(16)
                .collect::<String>()
                .to_ascii_lowercase()
        })
        .filter(|extension| !extension.is_empty())
        .unwrap_or_else(|| "bin".to_string());
    format!("{stem}.{extension}")
}

fn validate_entry_name(name: &str) -> Result<(), ProjectArchiveError> {
    if name.is_empty()
        || name.len() > MAX_ENTRY_NAME_BYTES
        || name.starts_with('/')
        || name.starts_with('\\')
        || name.contains('\\')
        || name.contains(':')
        || name.chars().any(|character| character.is_control())
    {
        return Err(ProjectArchiveError::UnsafeEntryName(name.to_string()));
    }

    let path = Path::new(name);
    for component in path.components() {
        let Component::Normal(component) = component else {
            return Err(ProjectArchiveError::UnsafeEntryName(name.to_string()));
        };
        let component = component.to_string_lossy();
        if component == "."
            || component == ".."
            || component.ends_with(' ')
            || component.ends_with('.')
            || is_windows_reserved_component(&component)
        {
            return Err(ProjectArchiveError::UnsafeEntryName(name.to_string()));
        }
    }
    Ok(())
}

fn is_windows_reserved_component(component: &str) -> bool {
    let stem = component
        .split('.')
        .next()
        .unwrap_or(component)
        .to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || stem.strip_prefix("COM").is_some_and(|suffix| {
            matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
        })
        || stem.strip_prefix("LPT").is_some_and(|suffix| {
            matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
        })
}

fn extraction_path(root: &Path, entry_name: &str) -> Result<PathBuf, ProjectArchiveError> {
    validate_entry_name(entry_name)?;
    let mut output = root.to_path_buf();
    for component in Path::new(entry_name).components() {
        match component {
            Component::Normal(component) => output.push(component),
            _ => return Err(ProjectArchiveError::UnsafeEntryName(entry_name.to_string())),
        }
    }
    if !output.starts_with(root) {
        return Err(ProjectArchiveError::UnsafeEntryName(entry_name.to_string()));
    }
    Ok(output)
}

fn create_extraction_guard() -> Result<ExtractionGuard, ProjectArchiveError> {
    let base = project_extraction_base()?;
    fs::create_dir_all(&base)?;
    for _ in 0..128 {
        let path = base.join(format!("open-{}", unique_suffix()));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(ExtractionGuard { path }),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(ProjectArchiveError::Io(error)),
        }
    }
    Err(ProjectArchiveError::Io(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not allocate a unique project extraction directory",
    )))
}

fn project_extraction_base() -> Result<PathBuf, ProjectArchiveError> {
    let executable = std::env::current_exe().map_err(ProjectArchiveError::Io)?;
    Ok(project_extraction_process_base(
        &executable,
        std::process::id(),
    ))
}

fn project_extraction_base_for_executable(executable: &Path) -> PathBuf {
    executable
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .join("coquerythmo-temp")
}

fn project_extraction_process_base(executable: &Path, process_id: u32) -> PathBuf {
    project_extraction_base_for_executable(executable).join(format!("process-{process_id}"))
}

/// Remove leftovers for this process id without touching another running
/// Coquerythmo instance (for example the DA and actor clients).
pub fn cleanup_project_extraction_at_startup() -> io::Result<()> {
    let executable = std::env::current_exe()?;
    cleanup_process_extraction(&executable, std::process::id())
}

fn cleanup_process_extraction(executable: &Path, process_id: u32) -> io::Result<()> {
    let current_base = project_extraction_process_base(executable, process_id);
    match fs::remove_dir_all(current_base) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    Ok(())
}

fn create_temporary_file_near(path: &Path) -> Result<(PathBuf, File), ProjectArchiveError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let stem = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("project.coquerythmo");
    for _ in 0..128 {
        let temporary_path = parent.join(format!(".{stem}.{}.tmp", unique_suffix()));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)
        {
            Ok(file) => return Ok((temporary_path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(ProjectArchiveError::Io(error)),
        }
    }
    Err(ProjectArchiveError::Io(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not allocate a unique temporary project file",
    )))
}

fn commit_temporary_file(temporary_path: &Path, destination: &Path) -> io::Result<()> {
    if destination.exists() && !destination.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "project destination is not a regular file",
        ));
    }

    match fs::rename(temporary_path, destination) {
        Ok(()) => return Ok(()),
        Err(error) if !destination.exists() => return Err(error),
        Err(_) => {}
    }

    // Windows does not replace an existing destination with `rename`. Rotate
    // the old file aside, install the complete temporary file, then remove the
    // backup. If installation fails, restore the original.
    let parent = destination.parent().unwrap_or(Path::new("."));
    let backup = parent.join(format!(".coquerythmo-backup-{}.tmp", unique_suffix()));
    fs::rename(destination, &backup)?;
    match fs::rename(temporary_path, destination) {
        Ok(()) => {
            let _ = fs::remove_file(backup);
            Ok(())
        }
        Err(error) => {
            let _ = fs::rename(&backup, destination);
            Err(error)
        }
    }
}

fn unique_suffix() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let counter = UNIQUE_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{}-{nanos:x}-{counter:x}", std::process::id())
}

fn write_u32<W: Write>(writer: &mut W, value: u32) -> io::Result<()> {
    writer.write_all(&value.to_le_bytes())
}

fn write_u64<W: Write>(writer: &mut W, value: u64) -> io::Result<()> {
    writer.write_all(&value.to_le_bytes())
}

fn read_u32<R: Read>(reader: &mut R) -> io::Result<u32> {
    let mut bytes = [0_u8; 4];
    reader.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_u64<R: Read>(reader: &mut R) -> io::Result<u64> {
    let mut bytes = [0_u8; 8];
    reader.read_exact(&mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}

struct Crc32(u32);

impl Crc32 {
    fn new() -> Self {
        Self(u32::MAX)
    }

    fn update(&mut self, bytes: &[u8]) {
        let table = crc32_table();
        for &byte in bytes {
            let index = ((self.0 ^ u32::from(byte)) & 0xff) as usize;
            self.0 = table[index] ^ (self.0 >> 8);
        }
    }

    fn finish(self) -> u32 {
        !self.0
    }
}

fn crc32_table() -> &'static [u32; 256] {
    static TABLE: OnceLock<[u32; 256]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = [0_u32; 256];
        for (index, slot) in table.iter_mut().enumerate() {
            let mut crc = index as u32;
            for _ in 0..8 {
                crc = if crc & 1 != 0 {
                    0xedb8_8320_u32 ^ (crc >> 1)
                } else {
                    crc >> 1
                };
            }
            *slot = crc;
        }
        table
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::integrity::{digest_to_hex, sha1_bytes};
    use crate::project::ProjectSettings;

    #[test]
    fn extraction_directory_lives_beside_the_executable() {
        let executable = Path::new("installation").join("coquerythmo.exe");
        assert_eq!(
            project_extraction_base_for_executable(&executable),
            Path::new("installation").join("coquerythmo-temp")
        );
        assert_eq!(
            project_extraction_process_base(&executable, 42),
            Path::new("installation")
                .join("coquerythmo-temp")
                .join("process-42")
        );
    }

    #[test]
    fn startup_cleanup_preserves_other_running_instances() {
        let root = std::env::temp_dir().join(format!(
            "coquerythmo-extraction-cleanup-{}",
            unique_suffix()
        ));
        let executable = root.join("coquerythmo.exe");
        let own = project_extraction_process_base(&executable, 41);
        let other = project_extraction_process_base(&executable, 42);
        fs::create_dir_all(&own).unwrap();
        fs::create_dir_all(&other).unwrap();

        cleanup_process_extraction(&executable, 41).unwrap();

        assert!(!own.exists());
        assert!(other.exists());
        let _ = fs::remove_dir_all(root);
    }
    use crate::project_metadata::TransactionJournal;
    use crate::recording::{
        AudioAsset, AudioAssetId, AudioClip, AudioClipId, AudioTrack, AudioTrackId,
        RecordingOperation, WaveformData,
    };

    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("coquerythmo-archive-test-{}", unique_suffix()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn path(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn sample_project(instrumental: Option<&Path>) -> Project {
        let mut project = Project::new();
        project.add_line_full(
            12,
            48,
            0.5,
            "Bonjour".into(),
            "Alice".into(),
            [0.2, 0.4, 0.8, 1.0],
        );
        project.set_settings(ProjectSettings {
            instrumental_audio_path: instrumental.map(|path| path.to_string_lossy().into_owned()),
            source_audio_offset_frames: 3,
            instrumental_audio_offset_frames: -2,
            ..ProjectSettings::default()
        });
        project
    }

    #[test]
    fn bundle_round_trip_embeds_and_extracts_every_current_asset() {
        let dir = TestDir::new();
        let source = dir.path("source.mp4");
        let proxy = dir.path("proxy.mp4");
        let instrumental = dir.path("instrumental.wav");
        let font = dir.path("font.ttf");
        let bundle = dir.path("project.coquerythmo");
        fs::write(&source, b"source-video-bytes").unwrap();
        fs::write(&proxy, b"proxy-video-bytes").unwrap();
        fs::write(&instrumental, b"instrumental-audio-bytes").unwrap();
        fs::write(&font, b"font-bytes").unwrap();

        let project = sample_project(Some(&instrumental));
        let line_id = project.lines().next().unwrap().id;
        let active_language_id = project.active_language_id().to_string();
        save_bundle(&project, 24.0, &bundle, &source, Some(&proxy), Some(&font)).unwrap();

        let loaded = load_project_file(&bundle).unwrap();
        assert_eq!(loaded.kind, ProjectFileKind::Bundle);
        assert_eq!(loaded.project_data.source_fps, 24.0);
        assert_eq!(loaded.project_data.lines.len(), 1);
        assert_eq!(loaded.project_data.lines[0].id, Some(line_id));
        assert_eq!(loaded.project_data.lines[0].text, "Bonjour");
        assert!(loaded.huuid.is_some());
        assert!(loaded.recording.is_none());
        assert_eq!(
            fs::read(loaded.source_video_path.as_ref().unwrap()).unwrap(),
            b"source-video-bytes"
        );
        assert_eq!(
            fs::read(loaded.proxy_video_path.as_ref().unwrap()).unwrap(),
            b"proxy-video-bytes"
        );
        assert_eq!(
            fs::read(loaded.font_asset_path.as_ref().unwrap()).unwrap(),
            b"font-bytes"
        );
        let extracted_instrumental = loaded
            .instrumental_audio_paths
            .get(&active_language_id)
            .unwrap();
        assert_eq!(
            fs::read(extracted_instrumental).unwrap(),
            b"instrumental-audio-bytes"
        );
        assert_eq!(
            loaded
                .project_data
                .settings
                .instrumental_audio_path
                .as_deref(),
            Some(extracted_instrumental.to_string_lossy().as_ref())
        );
        assert!(!loaded
            .project_data
            .settings
            .instrumental_audio_path
            .as_deref()
            .unwrap()
            .contains(dir.0.to_string_lossy().as_ref()));
    }

    #[test]
    fn every_successful_save_gets_a_new_huuid() {
        let dir = TestDir::new();
        let source = dir.path("source.mp4");
        let bundle = dir.path("identity.coquerythmo");
        fs::write(&source, b"video").unwrap();
        let project = sample_project(None);

        let first =
            save_bundle_with_metadata(&project, 24.0, &bundle, &source, None, None, None).unwrap();
        let first_loaded = load_project_file(&bundle).unwrap();
        assert_eq!(first_loaded.huuid.as_ref(), Some(&first.huuid));
        drop(first_loaded);

        let second =
            save_bundle_with_metadata(&project, 24.0, &bundle, &source, None, None, None).unwrap();
        assert_ne!(first.huuid, second.huuid);
        let second_loaded = load_project_file(&bundle).unwrap();
        assert_eq!(second_loaded.huuid.as_ref(), Some(&second.huuid));
    }

    #[test]
    fn bundle_round_trip_preserves_a_valid_transaction_journal() {
        let dir = TestDir::new();
        let source = dir.path("source.mp4");
        let bundle = dir.path("journal.coquerythmo");
        fs::write(&source, b"video").unwrap();
        let mut project = sample_project(None);
        let line_id = project.lines().next().unwrap().id;
        let language_id = project.active_language_id();
        let mut journal = TransactionJournal::from_project(&project, 24.0).unwrap();
        let command = Command::UpdateLineText {
            line_id,
            old_text: "Bonjour".into(),
            new_text: "Bonsoir".into(),
            old_emotions: Vec::new(),
            new_emotions: Vec::new(),
        };
        journal.append(language_id, command.clone()).unwrap();
        command.apply(&mut project);

        let saved =
            save_bundle_with_metadata(&project, 24.0, &bundle, &source, None, None, Some(&journal))
                .unwrap();
        let loaded = load_project_file(&bundle).unwrap();
        assert_eq!(loaded.huuid.as_ref(), Some(&saved.huuid));
        let restored_journal = loaded.transaction_journal.as_ref().unwrap();
        assert_eq!(
            saved
                .transaction_journal
                .as_ref()
                .unwrap()
                .checkpoint_hash(),
            restored_journal.checkpoint_hash()
        );
        restored_journal.validate_integrity().unwrap();
        assert_eq!(
            restored_journal
                .replay(24.0)
                .unwrap()
                .get_line(line_id)
                .unwrap()
                .text,
            "Bonsoir"
        );
    }

    #[test]
    fn bundle_round_trip_preserves_recording_state_log_and_flac_assets() {
        let dir = TestDir::new();
        let source = dir.path("source.mp4");
        let flac = dir.path("take-2.flac");
        let bundle = dir.path("recording.coquerythmo");
        let flac_bytes = b"fLaC\0portable-test-audio";
        fs::write(&source, b"video").unwrap();
        fs::write(&flac, flac_bytes).unwrap();

        let track_id = AudioTrackId::new(1);
        let asset_id = AudioAssetId::new(2);
        let clip_id = AudioClipId::new(3);
        let mut recording_project = RecordingProject::new(24.0).unwrap();
        let mut transaction_log = TransactionLog::default();
        transaction_log
            .append_and_apply(
                &mut recording_project,
                RecordingOperation::AddTrack {
                    track: AudioTrack::new(track_id, "Alice"),
                },
            )
            .unwrap();
        transaction_log
            .append_and_apply(
                &mut recording_project,
                RecordingOperation::AddAsset {
                    asset: AudioAsset {
                        id: asset_id,
                        file_name: "take-2.flac".into(),
                        sample_rate: 48_000,
                        channels: 1,
                        sample_count: 96_000,
                        checksum: digest_to_hex(sha1_bytes(flac_bytes)),
                        waveform: WaveformData {
                            samples_per_peak: 48_000,
                            peaks: vec![0.5, 0.25],
                        },
                    },
                },
            )
            .unwrap();
        transaction_log
            .append_and_apply(
                &mut recording_project,
                RecordingOperation::AddClip {
                    clip: AudioClip {
                        id: clip_id,
                        asset_id,
                        track_id,
                        start_frame: 12,
                        source_start_frame: 0,
                        duration_frames: 24,
                    },
                },
            )
            .unwrap();

        let asset_inputs = [RecordingAssetInput {
            asset_id,
            path: &flac,
        }];
        save_bundle_with_recording_data(
            &sample_project(None),
            24.0,
            &bundle,
            &source,
            None,
            None,
            None,
            Some(RecordingBundleInput {
                project: &recording_project,
                transaction_log: &transaction_log,
                assets: &asset_inputs,
            }),
        )
        .unwrap();

        let loaded = load_project_file(&bundle).unwrap();
        let recording = loaded.recording.as_ref().unwrap();
        assert_eq!(recording.project, recording_project);
        assert_eq!(recording.transaction_log, transaction_log);
        recording.transaction_log.verify_integrity().unwrap();
        assert_eq!(
            recording.project.asset(asset_id).unwrap().file_name,
            "take-2.flac"
        );
        assert_eq!(
            fs::read(&recording.audio_asset_paths[&asset_id]).unwrap(),
            flac_bytes
        );
        assert!(recording.audio_asset_paths[&asset_id].starts_with(
            loaded
                .extraction_root()
                .expect("recording FLAC must be extracted")
        ));
    }

    #[test]
    fn recording_save_rejects_a_flac_that_does_not_match_its_sha1() {
        let dir = TestDir::new();
        let source = dir.path("source.mp4");
        let flac = dir.path("take.flac");
        let bundle = dir.path("recording.coquerythmo");
        fs::write(&source, b"video").unwrap();
        fs::write(&flac, b"changed-audio").unwrap();
        fs::write(&bundle, b"previous-project").unwrap();

        let asset_id = AudioAssetId::new(1);
        let mut recording_project = RecordingProject::new(24.0).unwrap();
        let mut transaction_log = TransactionLog::default();
        transaction_log
            .append_and_apply(
                &mut recording_project,
                RecordingOperation::AddAsset {
                    asset: AudioAsset {
                        id: asset_id,
                        file_name: "take.flac".into(),
                        sample_rate: 48_000,
                        channels: 1,
                        sample_count: 48_000,
                        checksum: digest_to_hex(sha1_bytes(b"different-audio")),
                        waveform: WaveformData::default(),
                    },
                },
            )
            .unwrap();
        let assets = [RecordingAssetInput {
            asset_id,
            path: &flac,
        }];
        let error = match save_bundle_with_recording_data(
            &sample_project(None),
            24.0,
            &bundle,
            &source,
            None,
            None,
            None,
            Some(RecordingBundleInput {
                project: &recording_project,
                transaction_log: &transaction_log,
                assets: &assets,
            }),
        ) {
            Ok(_) => panic!("mismatched recording checksum unexpectedly saved"),
            Err(error) => error,
        };

        assert!(matches!(
            error,
            ProjectArchiveError::RecordingChecksumMismatch(id) if id == asset_id
        ));
        assert_eq!(fs::read(&bundle).unwrap(), b"previous-project");
    }

    #[test]
    fn recording_load_rejects_a_valid_container_with_wrong_flac_sha1() {
        let dir = TestDir::new();
        let bundle = dir.path("recording-checksum.coquerythmo");
        let expected_audio = b"fLaC-expected";
        let stored_audio = b"fLaC-different";
        let asset_id = AudioAssetId::new(1);
        let mut recording_project = RecordingProject::new(24.0).unwrap();
        let mut transaction_log = TransactionLog::default();
        transaction_log
            .append_and_apply(
                &mut recording_project,
                RecordingOperation::AddAsset {
                    asset: AudioAsset {
                        id: asset_id,
                        file_name: "take.flac".into(),
                        sample_rate: 48_000,
                        channels: 1,
                        sample_count: 48_000,
                        checksum: digest_to_hex(sha1_bytes(expected_audio)),
                        waveform: WaveformData::default(),
                    },
                },
            )
            .unwrap();
        let source_entry = "media/source.mp4";
        let recording_entry = "audio/recordings/0000000000000001-take.flac";
        let manifest = BundleManifest {
            format: FORMAT_NAME.into(),
            format_version: FORMAT_VERSION,
            huuid: None,
            transactions: None,
            project: ProjectData::from_project(&sample_project(None), 24.0),
            assets: BundleAssets {
                source_video: source_entry.into(),
                proxy_video: None,
                font: None,
                instrumentals: Vec::new(),
            },
            recording: Some(BundleRecordingManifest {
                project: recording_project,
                transaction_log,
                assets: vec![RecordingAssetManifest {
                    asset_id,
                    entry: recording_entry.into(),
                }],
            }),
        };
        let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
        let mut writer = BufWriter::new(File::create(&bundle).unwrap());
        writer.write_all(MAGIC).unwrap();
        write_u32(&mut writer, FORMAT_VERSION).unwrap();
        write_u32(&mut writer, 3).unwrap();
        write_bytes_entry(&mut writer, MANIFEST_ENTRY, &manifest_bytes).unwrap();
        write_bytes_entry(&mut writer, source_entry, b"video").unwrap();
        write_bytes_entry(&mut writer, recording_entry, stored_audio).unwrap();
        writer.write_all(FOOTER_MAGIC).unwrap();
        writer.flush().unwrap();

        let error = match load_project_file(&bundle) {
            Ok(_) => panic!("mismatched recording checksum unexpectedly loaded"),
            Err(error) => error,
        };
        assert!(matches!(
            error,
            ProjectArchiveError::RecordingChecksumMismatch(id) if id == asset_id
        ));
    }

    #[test]
    fn recording_save_rejects_state_not_reconstructed_by_its_log_cursor() {
        let dir = TestDir::new();
        let source = dir.path("source.mp4");
        let bundle = dir.path("ambiguous-recording.coquerythmo");
        fs::write(&source, b"video").unwrap();
        let mut recording_project = RecordingProject::new(24.0).unwrap();
        recording_project
            .apply(&RecordingOperation::AddTrack {
                track: AudioTrack::new(AudioTrackId::new(1), "Unlogged"),
            })
            .unwrap();
        let transaction_log = TransactionLog::default();

        let error = match save_bundle_with_recording_data(
            &sample_project(None),
            24.0,
            &bundle,
            &source,
            None,
            None,
            None,
            Some(RecordingBundleInput {
                project: &recording_project,
                transaction_log: &transaction_log,
                assets: &[],
            }),
        ) {
            Ok(_) => panic!("recording state without transactions unexpectedly saved"),
            Err(error) => error,
        };
        assert!(matches!(
            error,
            ProjectArchiveError::InvalidRecordingTransactionLog(_)
        ));
        assert!(!bundle.exists());
    }

    #[test]
    fn multilingual_helper_round_trips_all_instrumentals() {
        let dir = TestDir::new();
        let source = dir.path("source.mov");
        let french = dir.path("fr.wav");
        let japanese = dir.path("ja.flac");
        let bundle = dir.path("languages.coquerythmo");
        fs::write(&source, b"video").unwrap();
        fs::write(&french, b"fr-audio").unwrap();
        fs::write(&japanese, b"ja-audio").unwrap();
        let project = sample_project(None);
        let instrumentals = [
            InstrumentalAssetInput {
                language_id: "fr-FR",
                path: &french,
            },
            InstrumentalAssetInput {
                language_id: "日本語",
                path: &japanese,
            },
        ];

        save_bundle_with_instrumentals(
            &project,
            25.0,
            &bundle,
            &source,
            &instrumentals,
            None,
            None,
        )
        .unwrap();
        let loaded = load_project_file(&bundle).unwrap();
        assert_eq!(loaded.instrumental_audio_paths.len(), 2);
        assert_eq!(
            fs::read(&loaded.instrumental_audio_paths["fr-FR"]).unwrap(),
            b"fr-audio"
        );
        assert_eq!(
            fs::read(&loaded.instrumental_audio_paths["日本語"]).unwrap(),
            b"ja-audio"
        );
    }

    #[test]
    fn save_bundle_discovers_and_reinjects_project_language_instrumentals() {
        let dir = TestDir::new();
        let source = dir.path("source.mp4");
        let french = dir.path("fr.wav");
        let english = dir.path("en.wav");
        let bundle = dir.path("project-languages.coquerythmo");
        fs::write(&source, b"video").unwrap();
        fs::write(&french, b"fr-audio").unwrap();
        fs::write(&english, b"en-audio").unwrap();

        let mut project = sample_project(Some(&french));
        let french_id = project.active_language_id();
        let english_id = project.create_language("English", "en-GB");
        let mut english_settings = project.settings().clone();
        english_settings.instrumental_audio_path = Some(english.to_string_lossy().into_owned());
        project.set_settings(english_settings);

        save_bundle(&project, 24.0, &bundle, &source, None, None).unwrap();
        let loaded = load_project_file(&bundle).unwrap();

        let french_key = french_id.to_string();
        let english_key = english_id.to_string();
        assert_eq!(loaded.instrumental_audio_paths.len(), 2);
        assert_eq!(
            fs::read(&loaded.instrumental_audio_paths[&french_key]).unwrap(),
            b"fr-audio"
        );
        assert_eq!(
            fs::read(&loaded.instrumental_audio_paths[&english_key]).unwrap(),
            b"en-audio"
        );
        assert_eq!(loaded.project_data.active_language_id, Some(english_id));
        assert_eq!(
            loaded.project_data.settings.instrumental_audio_path,
            Some(
                loaded.instrumental_audio_paths[&english_key]
                    .to_string_lossy()
                    .into_owned()
            )
        );
        for language in &loaded.project_data.languages {
            let expected = loaded
                .instrumental_audio_paths
                .get(&language.id.to_string())
                .map(|path| path.to_string_lossy().into_owned());
            assert_eq!(language.project.settings.instrumental_audio_path, expected);
        }
    }

    #[test]
    fn legacy_json_fixture_remains_readable() {
        let dir = TestDir::new();
        let json_path = dir.path("legacy.json");
        fs::write(
            &json_path,
            include_bytes!("../tests/fixtures/project-small.json"),
        )
        .unwrap();

        let loaded = load_project_file(&json_path).unwrap();
        assert!(loaded.is_legacy_json());
        assert!(loaded.huuid.is_none());
        assert!(loaded.transaction_journal.is_none());
        assert!(loaded.recording.is_none());
        assert!(loaded.source_video_path.is_none());
        assert!(loaded.extraction_root().is_none());
        assert_eq!(loaded.project_data.source_fps, 24.0);
        assert_eq!(loaded.project_data.lines[0].text, "Bonjour");
        assert!(loaded.project_data.languages.is_empty());
        assert!(loaded
            .project_data
            .settings
            .instrumental_audio_path
            .is_none());
    }

    #[test]
    fn extracted_assets_are_removed_when_loaded_project_is_dropped() {
        let dir = TestDir::new();
        let source = dir.path("source.mp4");
        let bundle = dir.path("cleanup.coquerythmo");
        fs::write(&source, b"video").unwrap();
        save_bundle(&sample_project(None), 24.0, &bundle, &source, None, None).unwrap();
        let loaded = load_project_file(&bundle).unwrap();
        let extraction_root = loaded.extraction_root().unwrap().to_path_buf();
        assert!(extraction_root.exists());
        drop(loaded);
        assert!(!extraction_root.exists());
    }

    #[test]
    fn traversal_in_manifest_is_rejected_before_extraction() {
        let dir = TestDir::new();
        let bundle = dir.path("traversal.coquerythmo");
        let manifest = BundleManifest {
            format: FORMAT_NAME.into(),
            format_version: FORMAT_VERSION,
            huuid: None,
            transactions: None,
            project: ProjectData::from_project(&sample_project(None), 24.0),
            assets: BundleAssets {
                source_video: "../escape.mp4".into(),
                proxy_video: None,
                font: None,
                instrumentals: Vec::new(),
            },
            recording: None,
        };
        let bytes = serde_json::to_vec(&manifest).unwrap();
        let mut writer = BufWriter::new(File::create(&bundle).unwrap());
        writer.write_all(MAGIC).unwrap();
        write_u32(&mut writer, FORMAT_VERSION).unwrap();
        write_u32(&mut writer, 2).unwrap();
        write_bytes_entry(&mut writer, MANIFEST_ENTRY, &bytes).unwrap();
        write_bytes_entry(&mut writer, "safe.mp4", b"video").unwrap();
        writer.write_all(FOOTER_MAGIC).unwrap();
        writer.flush().unwrap();

        let error = match load_project_file(&bundle) {
            Ok(_) => panic!("unsafe bundle unexpectedly loaded"),
            Err(error) => error,
        };
        assert!(matches!(error, ProjectArchiveError::UnsafeEntryName(_)));
        assert!(!dir.path("escape.mp4").exists());
    }

    #[test]
    fn payload_corruption_is_detected() {
        let dir = TestDir::new();
        let source = dir.path("source.mp4");
        let bundle = dir.path("corrupt.coquerythmo");
        let marker = b"unique-source-payload-for-corruption";
        fs::write(&source, marker).unwrap();
        save_bundle(&sample_project(None), 24.0, &bundle, &source, None, None).unwrap();

        let mut bytes = fs::read(&bundle).unwrap();
        let position = bytes
            .windows(marker.len())
            .position(|window| window == marker)
            .expect("source payload in bundle");
        bytes[position] ^= 0x40;
        fs::write(&bundle, bytes).unwrap();

        let error = match load_project_file(&bundle) {
            Ok(_) => panic!("corrupt bundle unexpectedly loaded"),
            Err(error) => error,
        };
        assert!(matches!(error, ProjectArchiveError::ChecksumMismatch(_)));
    }

    #[test]
    fn failed_save_does_not_replace_an_existing_bundle() {
        let dir = TestDir::new();
        let destination = dir.path("existing.coquerythmo");
        fs::write(&destination, b"previous-project").unwrap();
        let missing_source = dir.path("missing.mp4");

        let error = save_bundle(
            &sample_project(None),
            24.0,
            &destination,
            &missing_source,
            None,
            None,
        )
        .unwrap_err();
        assert!(matches!(error, ProjectArchiveError::MissingAsset(_)));
        assert_eq!(fs::read(destination).unwrap(), b"previous-project");
    }

    #[test]
    fn destination_cannot_overwrite_an_asset_being_embedded() {
        let dir = TestDir::new();
        let source = dir.path("source.mp4");
        fs::write(&source, b"source-video").unwrap();

        let error =
            save_bundle(&sample_project(None), 24.0, &source, &source, None, None).unwrap_err();
        assert!(matches!(error, ProjectArchiveError::DestinationIsAsset(_)));
        assert_eq!(fs::read(source).unwrap(), b"source-video");
    }
}
