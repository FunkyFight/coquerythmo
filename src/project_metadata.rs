//! Portable project identity and transaction-journal metadata.
//!
//! These types contain no filesystem, UI, networking, or rendering concerns.
//! The archive adapter persists them, while application services decide when
//! to create journal entries and how to expose undo/redo.

use crate::command::Command;
use crate::export::ProjectData;
use crate::integrity::sha1_bytes;
use crate::project::{LanguageId, Project};
use rand::RngCore;
use serde::{de::Error as _, Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

const HUUID_PREFIX: &str = "Coquerythmo-";
const HUUID_TIMESTAMP_LEN: usize = 19;
pub const TRANSACTION_JOURNAL_VERSION: u32 = 1;

/// Identity of one successful saved representation of a project.
///
/// A HUUID deliberately changes on every save. It contains the application
/// version, a compact UTC timestamp, and an RFC 4122 UUIDv4.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Huuid(String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HuuidError(String);

impl Huuid {
    pub fn generate() -> Self {
        let mut uuid = [0_u8; 16];
        rand::thread_rng().fill_bytes(&mut uuid);
        Self::from_parts(env!("CARGO_PKG_VERSION"), SystemTime::now(), uuid)
            .expect("the current time and package version must form a valid HUUID")
    }

    fn from_parts(
        version: &str,
        saved_at: SystemTime,
        mut uuid: [u8; 16],
    ) -> Result<Self, HuuidError> {
        validate_version(version)?;
        uuid[6] = (uuid[6] & 0x0f) | 0x40;
        uuid[8] = (uuid[8] & 0x3f) | 0x80;
        let timestamp = format_utc_timestamp(saved_at)?;
        let uuid = format_uuid(uuid);
        Self::parse(&format!("{HUUID_PREFIX}{version}-{timestamp}-{uuid}"))
    }

    pub fn parse(value: &str) -> Result<Self, HuuidError> {
        if !value.starts_with(HUUID_PREFIX) {
            return Err(HuuidError("missing Coquerythmo prefix".into()));
        }
        if !value.is_ascii() {
            return Err(HuuidError(
                "HUUID must contain only ASCII characters".into(),
            ));
        }

        let uuid_start = value
            .len()
            .checked_sub(36)
            .ok_or_else(|| HuuidError("HUUID is too short".into()))?;
        let uuid_separator = uuid_start
            .checked_sub(1)
            .ok_or_else(|| HuuidError("HUUID is too short".into()))?;
        if value.as_bytes().get(uuid_separator) != Some(&b'-') {
            return Err(HuuidError("missing UUID separator".into()));
        }
        let timestamp_start = uuid_separator
            .checked_sub(HUUID_TIMESTAMP_LEN)
            .ok_or_else(|| HuuidError("HUUID is too short".into()))?;
        let timestamp_separator = timestamp_start
            .checked_sub(1)
            .ok_or_else(|| HuuidError("HUUID is too short".into()))?;
        if value.as_bytes().get(timestamp_separator) != Some(&b'-') {
            return Err(HuuidError("missing timestamp separator".into()));
        }

        let version = &value[HUUID_PREFIX.len()..timestamp_separator];
        validate_version(version)?;
        validate_timestamp(&value[timestamp_start..uuid_separator])?;
        validate_uuid(&value[uuid_start..])?;
        Ok(Self(value.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Huuid {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl fmt::Display for HuuidError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for HuuidError {}

impl FromStr for Huuid {
    type Err = HuuidError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl AsRef<str> for Huuid {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Serialize for Huuid {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Huuid {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(D::Error::custom)
    }
}

fn validate_version(version: &str) -> Result<(), HuuidError> {
    if version.is_empty()
        || !version
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+'))
    {
        return Err(HuuidError("invalid application version".into()));
    }
    Ok(())
}

fn validate_timestamp(timestamp: &str) -> Result<(), HuuidError> {
    let bytes = timestamp.as_bytes();
    if bytes.len() != HUUID_TIMESTAMP_LEN
        || bytes[8] != b'T'
        || bytes[18] != b'Z'
        || bytes
            .iter()
            .enumerate()
            .any(|(index, byte)| !matches!(index, 8 | 18) && !byte.is_ascii_digit())
    {
        return Err(HuuidError("invalid UTC timestamp".into()));
    }

    let number = |start: usize, end: usize| -> u32 {
        timestamp[start..end].parse::<u32>().unwrap_or_default()
    };
    let year = number(0, 4);
    let month = number(4, 6);
    let day = number(6, 8);
    let hour = number(9, 11);
    let minute = number(11, 13);
    let second = number(13, 15);
    let days_in_month = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    };
    if year == 0 || day == 0 || day > days_in_month || hour > 23 || minute > 59 || second > 59 {
        return Err(HuuidError("UTC timestamp is out of range".into()));
    }
    Ok(())
}

fn validate_uuid(uuid: &str) -> Result<(), HuuidError> {
    let bytes = uuid.as_bytes();
    if bytes.len() != 36
        || bytes[8] != b'-'
        || bytes[13] != b'-'
        || bytes[18] != b'-'
        || bytes[23] != b'-'
        || bytes
            .iter()
            .enumerate()
            .any(|(index, byte)| !matches!(index, 8 | 13 | 18 | 23) && !byte.is_ascii_hexdigit())
        || bytes[14] != b'4'
        || !matches!(bytes[19].to_ascii_lowercase(), b'8' | b'9' | b'a' | b'b')
    {
        return Err(HuuidError("invalid RFC 4122 UUIDv4".into()));
    }
    Ok(())
}

fn is_leap_year(year: u32) -> bool {
    year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
}

fn format_utc_timestamp(time: SystemTime) -> Result<String, HuuidError> {
    let duration = time
        .duration_since(UNIX_EPOCH)
        .map_err(|_| HuuidError("save time predates the Unix epoch".into()))?;
    let seconds = i64::try_from(duration.as_secs())
        .map_err(|_| HuuidError("save time is outside the supported range".into()))?;
    let days = seconds / 86_400;
    let seconds_in_day = seconds % 86_400;
    let (year, month, day) = civil_date_from_unix_days(days);
    if !(1..=9_999).contains(&year) {
        return Err(HuuidError(
            "save year is outside the supported range".into(),
        ));
    }
    let hour = seconds_in_day / 3_600;
    let minute = (seconds_in_day % 3_600) / 60;
    let second = seconds_in_day % 60;
    Ok(format!(
        "{year:04}{month:02}{day:02}T{hour:02}{minute:02}{second:02}{:03}Z",
        duration.subsec_millis()
    ))
}

// Howard Hinnant's civil-from-days algorithm. The input is a UTC day offset
// from 1970-01-01 and the output uses the proleptic Gregorian calendar.
fn civil_date_from_unix_days(days: i64) -> (i64, i64, i64) {
    let shifted = days + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year, month, day)
}

fn format_uuid(bytes: [u8; 16]) -> String {
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    )
}

/// SHA-1 digest used to identify a journal checkpoint or transaction prefix.
/// SHA-1 is used for deterministic corruption detection, not authentication.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct IntegrityHash([u8; 20]);

impl IntegrityHash {
    pub fn to_hex(self) -> String {
        let mut output = String::with_capacity(40);
        for byte in self.0 {
            use fmt::Write as _;
            let _ = write!(output, "{byte:02x}");
        }
        output
    }

    fn from_hex(value: &str) -> Result<Self, String> {
        if value.len() != 40 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("integrity hash must contain 40 hexadecimal characters".into());
        }
        let mut bytes = [0_u8; 20];
        for (index, slot) in bytes.iter_mut().enumerate() {
            *slot = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
                .map_err(|_| "invalid integrity hash")?;
        }
        Ok(Self(bytes))
    }
}

impl fmt::Display for IntegrityHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_hex())
    }
}

impl Serialize for IntegrityHash {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for IntegrityHash {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::from_hex(&value).map_err(D::Error::custom)
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct TransactionEntry {
    sequence: u64,
    language_id: LanguageId,
    previous_hash: IntegrityHash,
    payload: Command,
    hash: IntegrityHash,
}

impl TransactionEntry {
    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    pub fn language_id(&self) -> LanguageId {
        self.language_id
    }

    pub fn previous_hash(&self) -> IntegrityHash {
        self.previous_hash
    }

    pub fn payload(&self) -> &Command {
        &self.payload
    }

    pub fn hash(&self) -> IntegrityHash {
        self.hash
    }
}

/// Serializable transaction sequence with a stable checkpoint and applied
/// cursor. Entries after `cursor` form the redo branch.
#[derive(Clone, Serialize, Deserialize)]
pub struct TransactionJournal {
    schema_version: u32,
    checkpoint: ProjectData,
    checkpoint_hash: IntegrityHash,
    entries: Vec<TransactionEntry>,
    cursor: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransactionJournalError {
    UnsupportedVersion(u32),
    InvalidCheckpoint(String),
    CheckpointHashMismatch,
    CursorOutOfBounds { cursor: usize, len: usize },
    InvalidSequence { expected: u64, actual: u64 },
    PreviousHashMismatch { sequence: u64 },
    EntryHashMismatch { sequence: u64 },
    PrefixOutOfBounds { prefix: usize, len: usize },
    ReplayFpsMismatch,
    MissingLanguage(LanguageId),
    CommandPrecondition { sequence: u64, reason: String },
    CannotReplaceLast,
    Serialization(String),
}

impl fmt::Display for TransactionJournalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedVersion(version) => {
                write!(
                    formatter,
                    "unsupported transaction journal version {version}"
                )
            }
            Self::InvalidCheckpoint(reason) => write!(formatter, "invalid checkpoint: {reason}"),
            Self::CheckpointHashMismatch => formatter.write_str("checkpoint hash mismatch"),
            Self::CursorOutOfBounds { cursor, len } => {
                write!(
                    formatter,
                    "journal cursor {cursor} exceeds entry count {len}"
                )
            }
            Self::InvalidSequence { expected, actual } => {
                write!(
                    formatter,
                    "expected transaction sequence {expected}, got {actual}"
                )
            }
            Self::PreviousHashMismatch { sequence } => {
                write!(
                    formatter,
                    "previous hash mismatch at transaction {sequence}"
                )
            }
            Self::EntryHashMismatch { sequence } => {
                write!(
                    formatter,
                    "integrity hash mismatch at transaction {sequence}"
                )
            }
            Self::PrefixOutOfBounds { prefix, len } => {
                write!(formatter, "prefix {prefix} exceeds entry count {len}")
            }
            Self::ReplayFpsMismatch => {
                formatter.write_str("transaction replay FPS does not match the checkpoint timeline")
            }
            Self::MissingLanguage(language) => {
                write!(formatter, "transaction targets missing language {language}")
            }
            Self::CommandPrecondition { sequence, reason } => {
                write!(
                    formatter,
                    "transaction {sequence} cannot be replayed: {reason}"
                )
            }
            Self::CannotReplaceLast => formatter.write_str(
                "the last transaction can only be replaced at the end of the active branch",
            ),
            Self::Serialization(reason) => write!(formatter, "serialization failed: {reason}"),
        }
    }
}

impl std::error::Error for TransactionJournalError {}

impl TransactionJournal {
    pub fn new(checkpoint: ProjectData) -> Result<Self, TransactionJournalError> {
        validate_checkpoint(&checkpoint)?;
        let checkpoint_hash = hash_serializable(&checkpoint)?;
        Ok(Self {
            schema_version: TRANSACTION_JOURNAL_VERSION,
            checkpoint,
            checkpoint_hash,
            entries: Vec::new(),
            cursor: 0,
        })
    }

    pub fn from_project(project: &Project, fps: f64) -> Result<Self, TransactionJournalError> {
        Self::new(ProjectData::from_project(project, fps))
    }

    pub fn checkpoint(&self) -> &ProjectData {
        &self.checkpoint
    }

    pub fn checkpoint_hash(&self) -> IntegrityHash {
        self.checkpoint_hash
    }

    pub fn entries(&self) -> &[TransactionEntry] {
        &self.entries
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn append(
        &mut self,
        language_id: LanguageId,
        payload: Command,
    ) -> Result<&TransactionEntry, TransactionJournalError> {
        self.ensure_cursor_in_bounds()?;
        self.entries.truncate(self.cursor);
        let sequence = u64::try_from(self.cursor)
            .unwrap_or(u64::MAX)
            .saturating_add(1);
        let previous_hash = self.prefix_hash_at(self.cursor)?;
        let hash = transaction_hash(sequence, language_id, previous_hash, &payload)?;
        self.entries.push(TransactionEntry {
            sequence,
            language_id,
            previous_hash,
            payload,
            hash,
        });
        self.cursor += 1;
        Ok(self
            .entries
            .last()
            .expect("the appended transaction must be present"))
    }

    /// Replace a coalesced final command without creating a second entry.
    pub fn replace_last(
        &mut self,
        payload: Command,
    ) -> Result<&TransactionEntry, TransactionJournalError> {
        self.ensure_cursor_in_bounds()?;
        if self.cursor == 0 || self.cursor != self.entries.len() {
            return Err(TransactionJournalError::CannotReplaceLast);
        }
        let index = self.cursor - 1;
        let sequence = self.entries[index].sequence;
        let language_id = self.entries[index].language_id;
        let previous_hash = self.entries[index].previous_hash;
        let hash = transaction_hash(sequence, language_id, previous_hash, &payload)?;
        self.entries[index].payload = payload;
        self.entries[index].hash = hash;
        Ok(&self.entries[index])
    }

    pub fn set_cursor(&mut self, cursor: usize) -> Result<(), TransactionJournalError> {
        if cursor > self.entries.len() {
            return Err(TransactionJournalError::CursorOutOfBounds {
                cursor,
                len: self.entries.len(),
            });
        }
        self.cursor = cursor;
        Ok(())
    }

    pub fn undo_cursor(&mut self) -> Option<&TransactionEntry> {
        if self.cursor == 0 {
            return None;
        }
        self.cursor -= 1;
        self.entries.get(self.cursor)
    }

    pub fn redo_cursor(&mut self) -> Option<&TransactionEntry> {
        let entry = self.entries.get(self.cursor)?;
        self.cursor += 1;
        Some(entry)
    }

    pub fn entry_by_sequence(&self, sequence: u64) -> Option<&TransactionEntry> {
        self.entries
            .binary_search_by_key(&sequence, TransactionEntry::sequence)
            .ok()
            .and_then(|index| self.entries.get(index))
    }

    /// Hash identifying exactly `prefix` transactions after the checkpoint.
    pub fn prefix_hash_at(&self, prefix: usize) -> Result<IntegrityHash, TransactionJournalError> {
        if prefix > self.entries.len() {
            return Err(TransactionJournalError::PrefixOutOfBounds {
                prefix,
                len: self.entries.len(),
            });
        }
        Ok(if prefix == 0 {
            self.checkpoint_hash
        } else {
            self.entries[prefix - 1].hash
        })
    }

    pub fn matches_checkpoint(&self, checkpoint_hash: IntegrityHash) -> bool {
        self.checkpoint_hash == checkpoint_hash
    }

    /// Rewrite storage-specific checkpoint metadata and rebuild the hash chain.
    /// Archive adapters use this to replace machine-local asset paths without
    /// weakening validation of the resulting serialized journal.
    pub fn rewrite_checkpoint<F>(&mut self, rewrite: F) -> Result<(), TransactionJournalError>
    where
        F: FnOnce(&mut ProjectData),
    {
        rewrite(&mut self.checkpoint);
        validate_checkpoint(&self.checkpoint)?;
        self.checkpoint_hash = hash_serializable(&self.checkpoint)?;
        let mut previous_hash = self.checkpoint_hash;
        for entry in &mut self.entries {
            entry.previous_hash = previous_hash;
            entry.hash = transaction_hash(
                entry.sequence,
                entry.language_id,
                entry.previous_hash,
                &entry.payload,
            )?;
            previous_hash = entry.hash;
        }
        Ok(())
    }

    pub fn entries_after_prefix(
        &self,
        prefix: usize,
    ) -> Result<&[TransactionEntry], TransactionJournalError> {
        if prefix > self.entries.len() {
            return Err(TransactionJournalError::PrefixOutOfBounds {
                prefix,
                len: self.entries.len(),
            });
        }
        Ok(&self.entries[prefix..])
    }

    /// Locate the largest shared hash-chain prefix. Hash-chain divergence is
    /// monotonic, so the lookup itself is O(log n) after integrity validation.
    pub fn common_prefix_len(
        &self,
        other: &Self,
    ) -> Result<Option<usize>, TransactionJournalError> {
        self.validate_integrity()?;
        other.validate_integrity()?;
        if self.checkpoint_hash != other.checkpoint_hash {
            return Ok(None);
        }

        let mut low = 0_usize;
        let mut high = self.entries.len().min(other.entries.len());
        while low < high {
            let middle = low + (high - low).div_ceil(2);
            if self.prefix_hash_at(middle)? == other.prefix_hash_at(middle)? {
                low = middle;
            } else {
                high = middle - 1;
            }
        }
        Ok(Some(low))
    }

    pub fn validate_integrity(&self) -> Result<(), TransactionJournalError> {
        if self.schema_version != TRANSACTION_JOURNAL_VERSION {
            return Err(TransactionJournalError::UnsupportedVersion(
                self.schema_version,
            ));
        }
        validate_checkpoint(&self.checkpoint)?;
        if hash_serializable(&self.checkpoint)? != self.checkpoint_hash {
            return Err(TransactionJournalError::CheckpointHashMismatch);
        }
        self.ensure_cursor_in_bounds()?;

        let mut previous_hash = self.checkpoint_hash;
        for (index, entry) in self.entries.iter().enumerate() {
            let expected_sequence = u64::try_from(index).unwrap_or(u64::MAX).saturating_add(1);
            if entry.sequence != expected_sequence {
                return Err(TransactionJournalError::InvalidSequence {
                    expected: expected_sequence,
                    actual: entry.sequence,
                });
            }
            if entry.previous_hash != previous_hash {
                return Err(TransactionJournalError::PreviousHashMismatch {
                    sequence: entry.sequence,
                });
            }
            let expected_hash = transaction_hash(
                entry.sequence,
                entry.language_id,
                entry.previous_hash,
                &entry.payload,
            )?;
            if entry.hash != expected_hash {
                return Err(TransactionJournalError::EntryHashMismatch {
                    sequence: entry.sequence,
                });
            }
            previous_hash = entry.hash;
        }
        Ok(())
    }

    /// Verify the complete chain before rebuilding a project in an isolated
    /// value. A failure never partially mutates the caller's project.
    pub fn replay(&self, fps: f64) -> Result<Project, TransactionJournalError> {
        self.validate_integrity()?;
        if !fps.is_finite() || fps <= 0.0 || fps != self.checkpoint.source_fps {
            return Err(TransactionJournalError::ReplayFpsMismatch);
        }
        let mut project = Project::new();
        self.checkpoint
            .try_apply_to_project(&mut project, fps)
            .map_err(TransactionJournalError::InvalidCheckpoint)?;

        for entry in &self.entries[..self.cursor] {
            if project.active_language_id() != entry.language_id
                && !project.select_language(entry.language_id)
            {
                return Err(TransactionJournalError::MissingLanguage(entry.language_id));
            }
            validate_command_precondition(&entry.payload, &project).map_err(|reason| {
                TransactionJournalError::CommandPrecondition {
                    sequence: entry.sequence,
                    reason,
                }
            })?;
            entry.payload.apply(&mut project);
        }
        Ok(project)
    }

    fn ensure_cursor_in_bounds(&self) -> Result<(), TransactionJournalError> {
        if self.cursor > self.entries.len() {
            return Err(TransactionJournalError::CursorOutOfBounds {
                cursor: self.cursor,
                len: self.entries.len(),
            });
        }
        Ok(())
    }
}

fn validate_checkpoint(checkpoint: &ProjectData) -> Result<(), TransactionJournalError> {
    checkpoint
        .validate_line_ids()
        .map_err(TransactionJournalError::InvalidCheckpoint)?;
    if !checkpoint.has_stable_line_ids() {
        return Err(TransactionJournalError::InvalidCheckpoint(
            "transaction checkpoints require an id for every line".into(),
        ));
    }
    if !checkpoint.source_fps.is_finite() || checkpoint.source_fps <= 0.0 {
        return Err(TransactionJournalError::InvalidCheckpoint(
            "transaction checkpoints require a positive finite source FPS".into(),
        ));
    }
    if checkpoint.languages.is_empty() {
        return Err(TransactionJournalError::InvalidCheckpoint(
            "transaction checkpoints require stable language snapshots".into(),
        ));
    }
    let mut language_ids = std::collections::HashSet::new();
    for language in &checkpoint.languages {
        if !language_ids.insert(language.id) {
            return Err(TransactionJournalError::InvalidCheckpoint(format!(
                "duplicate language id {}",
                language.id
            )));
        }
        if language.name.trim().is_empty() {
            return Err(TransactionJournalError::InvalidCheckpoint(format!(
                "language {} has an empty name",
                language.id
            )));
        }
        if !language.project.source_fps.is_finite() || language.project.source_fps <= 0.0 {
            return Err(TransactionJournalError::InvalidCheckpoint(format!(
                "language {} has an invalid source FPS",
                language.id
            )));
        }
        if language.project.source_fps != checkpoint.source_fps {
            return Err(TransactionJournalError::InvalidCheckpoint(format!(
                "language {} uses a different source FPS",
                language.id
            )));
        }
        if !language.project.languages.is_empty() || language.project.active_language_id.is_some() {
            return Err(TransactionJournalError::InvalidCheckpoint(format!(
                "language {} contains nested language metadata",
                language.id
            )));
        }
    }
    let active_language = checkpoint.active_language_id.ok_or_else(|| {
        TransactionJournalError::InvalidCheckpoint(
            "transaction checkpoint has no active language id".into(),
        )
    })?;
    if !language_ids.contains(&active_language) {
        return Err(TransactionJournalError::InvalidCheckpoint(format!(
            "active language {active_language} is not present in the checkpoint"
        )));
    }
    Ok(())
}

fn transaction_hash(
    sequence: u64,
    language_id: LanguageId,
    previous_hash: IntegrityHash,
    payload: &Command,
) -> Result<IntegrityHash, TransactionJournalError> {
    hash_serializable(&(
        TRANSACTION_JOURNAL_VERSION,
        sequence,
        language_id,
        previous_hash,
        payload,
    ))
}

fn hash_serializable<T: Serialize + ?Sized>(
    value: &T,
) -> Result<IntegrityHash, TransactionJournalError> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| TransactionJournalError::Serialization(error.to_string()))?;
    Ok(IntegrityHash(sha1_bytes(&bytes)))
}

fn validate_command_precondition(command: &Command, project: &Project) -> Result<(), String> {
    let missing_line = |id: u64| format!("line {id} does not exist");
    match command {
        Command::CreateLine { snapshot, index } | Command::InsertLine { snapshot, index } => {
            snapshot.validate()?;
            if project.get_line(snapshot.id).is_some() {
                return Err(format!("line {} already exists", snapshot.id));
            }
            if *index > project.line_count() {
                return Err(format!("line insertion index {index} is out of bounds"));
            }
        }
        Command::InsertLines { lines } => {
            let mut ids = std::collections::HashSet::new();
            for (line, _) in lines {
                line.validate()?;
                if !ids.insert(line.id) || project.get_line(line.id).is_some() {
                    return Err(format!("line {} already exists", line.id));
                }
            }
            let mut indices: Vec<usize> = lines.iter().map(|(_, index)| *index).collect();
            indices.sort_unstable();
            for (offset, index) in indices.into_iter().enumerate() {
                if index > project.line_count().saturating_add(offset) {
                    return Err(format!("line insertion index {index} is out of bounds"));
                }
            }
        }
        Command::DeleteLine { snapshot, index } => {
            if project.get_line(snapshot.id) != Some(snapshot) {
                return Err(format!(
                    "line {} does not match the delete snapshot",
                    snapshot.id
                ));
            }
            if project.line_index(snapshot.id) != Some(*index) {
                return Err(format!(
                    "line {} does not match delete index {index}",
                    snapshot.id
                ));
            }
        }
        Command::DeleteLines { lines } => {
            let mut indices = std::collections::HashSet::new();
            for (line, index) in lines {
                if project.get_line(line.id) != Some(line) {
                    return Err(format!(
                        "line {} does not match the delete snapshot",
                        line.id
                    ));
                }
                if project.line_index(line.id) != Some(*index) || !indices.insert(*index) {
                    return Err(format!(
                        "line {} does not match delete index {index}",
                        line.id
                    ));
                }
            }
        }
        Command::SplitLine {
            old_line,
            old_index,
            first_line,
            second_line,
            second_index,
            ..
        } => {
            if project.get_line(old_line.id) != Some(old_line) {
                return Err(format!(
                    "line {} does not match the split snapshot",
                    old_line.id
                ));
            }
            if project.line_index(old_line.id) != Some(*old_index) {
                return Err(format!(
                    "line {} does not match split index {old_index}",
                    old_line.id
                ));
            }
            first_line.validate()?;
            second_line.validate()?;
            if first_line.id != old_line.id {
                return Err("the first split line changed the original line id".into());
            }
            if project.get_line(second_line.id).is_some() {
                return Err(format!("split line {} already exists", second_line.id));
            }
            if *second_index > project.line_count() {
                return Err(format!(
                    "second split line index {second_index} is out of bounds"
                ));
            }
        }
        Command::MoveLine {
            line_id,
            old_start,
            old_y_slot,
            ..
        } => {
            let line = project
                .get_line(*line_id)
                .ok_or_else(|| missing_line(*line_id))?;
            if line.start_frame != *old_start || line.y_slot != *old_y_slot {
                return Err(format!("line {line_id} does not match the move origin"));
            }
        }
        Command::MoveLines { moves } => {
            for movement in moves {
                let line = project
                    .get_line(movement.line_id)
                    .ok_or_else(|| missing_line(movement.line_id))?;
                if line.start_frame != movement.old_start || line.y_slot != movement.old_y_slot {
                    return Err(format!(
                        "line {} does not match the move origin",
                        movement.line_id
                    ));
                }
            }
        }
        Command::ResizeLine {
            line_id,
            old_start,
            old_dur,
            ..
        } => {
            let line = project
                .get_line(*line_id)
                .ok_or_else(|| missing_line(*line_id))?;
            if line.start_frame != *old_start || line.duration_frames != *old_dur {
                return Err(format!("line {line_id} does not match the resize origin"));
            }
        }
        Command::UpdateLineText {
            line_id, old_text, ..
        } => {
            let line = project
                .get_line(*line_id)
                .ok_or_else(|| missing_line(*line_id))?;
            if &line.text != old_text {
                return Err(format!("line {line_id} does not match the previous text"));
            }
        }
        Command::SetLineKaraoke {
            line_id,
            old_karaoke,
            old_ratios,
            ..
        } => {
            let line = project
                .get_line(*line_id)
                .ok_or_else(|| missing_line(*line_id))?;
            if line.karaoke != *old_karaoke || &line.syllable_ratios != old_ratios {
                return Err(format!("line {line_id} does not match the karaoke origin"));
            }
        }
        Command::SetSyllableRatios {
            line_id,
            old_ratios,
            ..
        } => {
            let line = project
                .get_line(*line_id)
                .ok_or_else(|| missing_line(*line_id))?;
            if &line.syllable_ratios != old_ratios {
                return Err(format!("line {line_id} does not match the previous ratios"));
            }
        }
        Command::SetCharacter {
            line_id,
            old_name,
            old_color,
            old_voice_actor_names,
            ..
        } => {
            let line = project
                .get_line(*line_id)
                .ok_or_else(|| missing_line(*line_id))?;
            if &line.character_name != old_name
                || line.character_color != *old_color
                || &line.voice_actor_names != old_voice_actor_names
            {
                return Err(format!(
                    "line {line_id} does not match the character origin"
                ));
            }
        }
        Command::SetCharacterColor {
            line_id, old_color, ..
        } => {
            let line = project
                .get_line(*line_id)
                .ok_or_else(|| missing_line(*line_id))?;
            if line.character_color != *old_color {
                return Err(format!("line {line_id} does not match the previous color"));
            }
        }
        Command::RenameCharacter {
            changes,
            old_known_characters,
            ..
        } => {
            if project.known_characters() != old_known_characters.as_slice() {
                return Err("known characters do not match the rename origin".into());
            }
            for change in changes {
                let line = project
                    .get_line(change.line_id)
                    .ok_or_else(|| missing_line(change.line_id))?;
                if line.character_name != change.old_name {
                    return Err(format!(
                        "line {} does not match the previous character name",
                        change.line_id
                    ));
                }
            }
        }
        Command::SetVoiceActors { changes } => {
            for change in changes {
                let line = project
                    .get_line(change.line_id)
                    .ok_or_else(|| missing_line(change.line_id))?;
                if line.voice_actor_names != change.old_voice_actor_names {
                    return Err(format!(
                        "line {} does not match the previous voice actors",
                        change.line_id
                    ));
                }
            }
        }
        Command::CreateVoiceActor { actor } => {
            if project
                .voice_actors()
                .iter()
                .any(|existing| existing.name == actor.name)
            {
                return Err(format!("voice actor {:?} already exists", actor.name));
            }
        }
        Command::AddMarker { index, .. } => {
            if *index > project.marker_count() {
                return Err(format!("marker insertion index {index} is out of bounds"));
            }
        }
        Command::RemoveMarker { marker, index } => {
            let current = project
                .marker(*index)
                .ok_or_else(|| format!("marker index {index} does not exist"))?;
            if current.kind != marker.kind || current.frame != marker.frame {
                return Err(format!("marker {index} does not match the remove snapshot"));
            }
        }
        Command::MoveMarker {
            index, old_frame, ..
        } => {
            let current = project
                .marker(*index)
                .ok_or_else(|| format!("marker index {index} does not exist"))?;
            if current.frame != *old_frame {
                return Err(format!("marker {index} does not match the move origin"));
            }
        }
        Command::UpdateLineNote {
            line_id, old_note, ..
        } => {
            let line = project
                .get_line(*line_id)
                .ok_or_else(|| missing_line(*line_id))?;
            if &line.note != old_note {
                return Err(format!("line {line_id} does not match the previous note"));
            }
        }
        Command::AddDrawingStroke { stroke } => {
            if project.drawing().get(stroke.id).is_some() {
                return Err(format!("drawing stroke {} already exists", stroke.id));
            }
        }
        Command::EraseDrawingStrokes { strokes } => {
            for stroke in strokes {
                if project.drawing().get(stroke.id) != Some(stroke) {
                    return Err(format!(
                        "drawing stroke {} does not match the erase snapshot",
                        stroke.id
                    ));
                }
            }
        }
        Command::TransformStrokes {
            stroke_ids,
            old_points,
            new_points,
        } => {
            if stroke_ids.len() != old_points.len() || stroke_ids.len() != new_points.len() {
                return Err("drawing transform arrays have different lengths".into());
            }
            for (stroke_id, points) in stroke_ids.iter().zip(old_points) {
                let stroke = project
                    .drawing()
                    .get(*stroke_id)
                    .ok_or_else(|| format!("drawing stroke {stroke_id} does not exist"))?;
                if &stroke.points != points {
                    return Err(format!(
                        "drawing stroke {stroke_id} does not match the transform origin"
                    ));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn project_with_line() -> (Project, u64) {
        let mut project = Project::new_with_language("Français", "fr");
        let line_id = project.add_line_full(
            0,
            48,
            0.5,
            "before".into(),
            "Alice".into(),
            [0.2, 0.4, 0.8, 1.0],
        );
        (project, line_id)
    }

    #[test]
    fn huuid_contains_utc_timestamp_and_rfc4122_v4_uuid() {
        let timestamp = UNIX_EPOCH + Duration::from_secs(1_709_164_800);
        let uuid = [
            0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x06, 0x77, 0x08, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
            0xee, 0xff,
        ];
        let huuid = Huuid::from_parts("3.6.0", timestamp, uuid).unwrap();
        assert_eq!(
            huuid.as_str(),
            "Coquerythmo-3.6.0-20240229T000000000Z-00112233-4455-4677-8899-aabbccddeeff"
        );
        assert_eq!(Huuid::parse(huuid.as_str()).unwrap(), huuid);
    }

    #[test]
    fn huuid_rejects_non_v4_or_out_of_range_timestamp() {
        assert!(Huuid::parse(
            "Coquerythmo-3.6.0-20240230T000000000Z-00112233-4455-6677-8899-aabbccddeeff"
        )
        .is_err());
        assert!(Huuid::parse(
            "Coquerythmo-3.6.0-20240229T000000000Z-00112233-4455-4677-7899-aabbccddeeff"
        )
        .is_err());
    }

    #[test]
    fn sha1_matches_standard_test_vector() {
        assert_eq!(
            IntegrityHash(sha1_bytes(b"abc")).to_hex(),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
    }

    #[test]
    fn journal_roundtrip_validates_and_replays_at_its_cursor() {
        let (project, line_id) = project_with_line();
        let language_id = project.active_language_id();
        let mut journal = TransactionJournal::from_project(&project, 24.0).unwrap();
        journal
            .append(
                language_id,
                Command::UpdateLineText {
                    line_id,
                    old_text: "before".into(),
                    new_text: "after".into(),
                },
            )
            .unwrap();

        let encoded = serde_json::to_vec(&journal).unwrap();
        let mut decoded: TransactionJournal = serde_json::from_slice(&encoded).unwrap();
        decoded.validate_integrity().unwrap();
        let replayed = decoded.replay(24.0).unwrap();
        assert_eq!(replayed.get_line(line_id).unwrap().text, "after");

        decoded.undo_cursor().unwrap();
        let replayed = decoded.replay(24.0).unwrap();
        assert_eq!(replayed.get_line(line_id).unwrap().text, "before");
        decoded.redo_cursor().unwrap();
        assert_eq!(decoded.cursor(), 1);
        assert!(matches!(
            decoded.replay(25.0),
            Err(TransactionJournalError::ReplayFpsMismatch)
        ));
    }

    #[test]
    fn journal_detects_payload_tampering_before_replay() {
        let (project, line_id) = project_with_line();
        let mut journal = TransactionJournal::from_project(&project, 24.0).unwrap();
        journal
            .append(
                project.active_language_id(),
                Command::UpdateLineText {
                    line_id,
                    old_text: "before".into(),
                    new_text: "after".into(),
                },
            )
            .unwrap();
        journal.entries[0].payload = Command::UpdateLineText {
            line_id,
            old_text: "before".into(),
            new_text: "tampered".into(),
        };

        assert!(matches!(
            journal.validate_integrity(),
            Err(TransactionJournalError::EntryHashMismatch { sequence: 1 })
        ));
        assert!(journal.replay(24.0).is_err());
    }

    #[test]
    fn common_prefix_lookup_finds_divergence_and_append_truncates_redo() {
        let (project, line_id) = project_with_line();
        let language_id = project.active_language_id();
        let mut left = TransactionJournal::from_project(&project, 24.0).unwrap();
        left.append(
            language_id,
            Command::UpdateLineText {
                line_id,
                old_text: "before".into(),
                new_text: "one".into(),
            },
        )
        .unwrap();
        let mut right = left.clone();
        left.append(
            language_id,
            Command::UpdateLineText {
                line_id,
                old_text: "one".into(),
                new_text: "left".into(),
            },
        )
        .unwrap();
        right
            .append(
                language_id,
                Command::UpdateLineText {
                    line_id,
                    old_text: "one".into(),
                    new_text: "right".into(),
                },
            )
            .unwrap();
        assert_eq!(left.common_prefix_len(&right).unwrap(), Some(1));

        left.undo_cursor();
        left.append(
            language_id,
            Command::UpdateLineText {
                line_id,
                old_text: "one".into(),
                new_text: "replacement".into(),
            },
        )
        .unwrap();
        assert_eq!(left.entries().len(), 2);
        assert_eq!(left.cursor(), 2);
        left.validate_integrity().unwrap();
    }

    #[test]
    fn replacing_a_coalesced_tip_rehashes_without_growing_the_journal() {
        let (project, line_id) = project_with_line();
        let mut journal = TransactionJournal::from_project(&project, 24.0).unwrap();
        journal
            .append(
                project.active_language_id(),
                Command::UpdateLineText {
                    line_id,
                    old_text: "before".into(),
                    new_text: "draft".into(),
                },
            )
            .unwrap();
        let draft_hash = journal.entries()[0].hash();
        journal
            .replace_last(Command::UpdateLineText {
                line_id,
                old_text: "before".into(),
                new_text: "final".into(),
            })
            .unwrap();

        assert_eq!(journal.entries().len(), 1);
        assert_ne!(journal.entries()[0].hash(), draft_hash);
        journal.validate_integrity().unwrap();
        assert_eq!(
            journal
                .replay(24.0)
                .unwrap()
                .get_line(line_id)
                .unwrap()
                .text,
            "final"
        );
    }

    #[test]
    fn transaction_checkpoint_rejects_duplicate_or_missing_ids() {
        let (project, _) = project_with_line();
        let mut checkpoint = ProjectData::from_project(&project, 24.0);
        let duplicate = checkpoint.languages[0].project.lines[0].clone();
        checkpoint.languages[0].project.lines.push(duplicate);
        assert!(matches!(
            TransactionJournal::new(checkpoint),
            Err(TransactionJournalError::InvalidCheckpoint(_))
        ));

        let mut duplicate_language = ProjectData::from_project(&project, 24.0);
        let language_copy = duplicate_language.languages[0].clone();
        duplicate_language.languages.push(language_copy);
        assert!(matches!(
            TransactionJournal::new(duplicate_language),
            Err(TransactionJournalError::InvalidCheckpoint(_))
        ));

        let legacy: ProjectData =
            serde_json::from_str(include_str!("../tests/fixtures/project-small.json")).unwrap();
        assert!(!legacy.has_stable_line_ids());
        assert!(TransactionJournal::new(legacy.clone()).is_err());
    }

    #[test]
    fn rewriting_checkpoint_rehashes_the_complete_chain() {
        let (project, line_id) = project_with_line();
        let mut journal = TransactionJournal::from_project(&project, 24.0).unwrap();
        journal
            .append(
                project.active_language_id(),
                Command::UpdateLineText {
                    line_id,
                    old_text: "before".into(),
                    new_text: "after".into(),
                },
            )
            .unwrap();
        let old_checkpoint_hash = journal.checkpoint_hash();
        let old_entry_hash = journal.entries()[0].hash();
        journal
            .rewrite_checkpoint(|checkpoint| {
                checkpoint.settings.instrumental_audio_path = Some("audio/portable.flac".into());
                for language in &mut checkpoint.languages {
                    language.project.settings.instrumental_audio_path =
                        Some("audio/portable.flac".into());
                }
            })
            .unwrap();

        assert_ne!(journal.checkpoint_hash(), old_checkpoint_hash);
        assert_ne!(journal.entries()[0].hash(), old_entry_hash);
        journal.validate_integrity().unwrap();
    }
}
