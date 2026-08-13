//! Audio-only voiceline cutting model and FFmpeg export adapter.

use crate::recording::{RecordedAudio, WaveformData};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

pub type AudioId = u64;
pub type RegionId = u64;

const MIN_REGION_MS: u64 = 20;
const AUTO_SILENCE_MS: u64 = 200;
const AUTO_PADDING_MS: u64 = 40;
const AUTO_THRESHOLD: f32 = 0.01;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NamingMode {
    Manual,
    Automatic { pattern: String, next: u32 },
}

impl Default for NamingMode {
    fn default() -> Self {
        Self::Automatic {
            pattern: default_automatic_pattern(),
            next: 1,
        }
    }
}

fn default_automatic_pattern() -> String {
    "voiceline_{num}".into()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Region {
    pub id: RegionId,
    pub name: String,
    pub start_ms: u64,
    pub end_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Audio {
    pub id: AudioId,
    pub source_path: PathBuf,
    #[serde(skip)]
    pub playback_path: PathBuf,
    pub file_name: String,
    pub sample_rate: u32,
    pub sample_count: u64,
    #[serde(skip)]
    pub waveform: WaveformData,
    #[serde(default)]
    pub regions: Vec<Region>,
}

impl Audio {
    pub fn duration_ms(&self) -> u64 {
        self.sample_count
            .saturating_mul(1_000)
            .checked_div(u64::from(self.sample_rate.max(1)))
            .unwrap_or(0)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoicelinesProject {
    #[serde(default)]
    audios: Vec<Audio>,
    #[serde(default)]
    active_audio: Option<AudioId>,
    #[serde(default)]
    naming: NamingMode,
    #[serde(default = "default_automatic_pattern")]
    automatic_pattern: String,
    #[serde(default = "first_id")]
    next_id: u64,
}

const fn first_id() -> u64 {
    1
}

impl Default for VoicelinesProject {
    fn default() -> Self {
        Self {
            audios: Vec::new(),
            active_audio: None,
            naming: NamingMode::default(),
            automatic_pattern: default_automatic_pattern(),
            next_id: first_id(),
        }
    }
}

impl VoicelinesProject {
    pub fn audios(&self) -> &[Audio] {
        &self.audios
    }

    pub fn active_audio_id(&self) -> Option<AudioId> {
        self.active_audio
    }

    pub fn active_audio(&self) -> Option<&Audio> {
        let id = self.active_audio?;
        self.audios.iter().find(|audio| audio.id == id)
    }

    pub fn audio(&self, id: AudioId) -> Option<&Audio> {
        self.audios.iter().find(|audio| audio.id == id)
    }

    pub fn naming(&self) -> &NamingMode {
        &self.naming
    }

    pub fn add_audio(
        &mut self,
        source_path: PathBuf,
        playback_path: PathBuf,
        recorded: RecordedAudio,
    ) -> AudioId {
        let id = self.allocate_id();
        let file_name = source_path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or(recorded.file_name);
        self.audios.push(Audio {
            id,
            source_path,
            playback_path,
            file_name,
            sample_rate: recorded.sample_rate,
            sample_count: recorded.sample_count,
            waveform: recorded.waveform,
            regions: Vec::new(),
        });
        self.active_audio = Some(id);
        id
    }

    pub fn remove_audio(&mut self, id: AudioId) -> bool {
        let Some(index) = self.audios.iter().position(|audio| audio.id == id) else {
            return false;
        };
        self.audios.remove(index);
        if self.active_audio == Some(id) {
            self.active_audio = self
                .audios
                .get(index.min(self.audios.len().saturating_sub(1)))
                .map(|audio| audio.id);
        }
        true
    }

    pub fn select_audio(&mut self, id: AudioId) -> bool {
        if self.active_audio == Some(id) || self.audio(id).is_none() {
            return false;
        }
        self.active_audio = Some(id);
        true
    }

    pub fn bind_audio(
        &mut self,
        id: AudioId,
        playback_path: PathBuf,
        recorded: RecordedAudio,
    ) -> bool {
        let Some(audio) = self.audios.iter_mut().find(|audio| audio.id == id) else {
            return false;
        };
        audio.playback_path = playback_path;
        audio.sample_rate = recorded.sample_rate;
        audio.sample_count = recorded.sample_count;
        audio.waveform = recorded.waveform;
        let duration = audio.duration_ms();
        audio
            .regions
            .retain(|region| valid_bounds(region.start_ms, region.end_ms, duration).is_some());
        true
    }

    pub fn set_manual_naming(&mut self) {
        self.naming = NamingMode::Manual;
    }

    pub fn set_automatic_naming(&mut self, pattern: &str) -> Result<(), String> {
        let mut pattern = clean_name(pattern);
        if pattern.is_empty() {
            return Err("Indiquez un nom de base".into());
        }
        if !pattern.contains("{num}") {
            pattern.push_str("_{num}");
        }
        self.automatic_pattern.clone_from(&pattern);
        self.naming = NamingMode::Automatic {
            pattern,
            next: self.next_automatic_number(),
        };
        Ok(())
    }

    pub fn automatic_pattern(&self) -> &str {
        &self.automatic_pattern
    }

    pub fn add_region(&mut self, start_ms: u64, end_ms: u64) -> Option<RegionId> {
        let duration_ms = self.active_audio()?.duration_ms();
        let (start_ms, end_ms) = valid_bounds(start_ms, end_ms, duration_ms)?;
        let name = self.allocate_region_name();
        let id = self.allocate_id();
        let audio = self.active_audio_mut()?;
        audio.regions.push(Region {
            id,
            name,
            start_ms,
            end_ms,
        });
        audio
            .regions
            .sort_by_key(|region| (region.start_ms, region.id));
        Some(id)
    }

    pub fn move_region(&mut self, id: RegionId, start_ms: u64, end_ms: u64) -> bool {
        let Some(audio) = self.active_audio_mut() else {
            return false;
        };
        let Some((start_ms, end_ms)) = valid_bounds(start_ms, end_ms, audio.duration_ms()) else {
            return false;
        };
        let Some(region) = audio.regions.iter_mut().find(|region| region.id == id) else {
            return false;
        };
        if (region.start_ms, region.end_ms) == (start_ms, end_ms) {
            return false;
        }
        region.start_ms = start_ms;
        region.end_ms = end_ms;
        audio
            .regions
            .sort_by_key(|region| (region.start_ms, region.id));
        true
    }

    pub fn rename_region(&mut self, id: RegionId, name: &str) -> bool {
        let name = clean_name(name);
        if name.is_empty() {
            return false;
        }
        let Some(region) = self
            .active_audio_mut()
            .and_then(|audio| audio.regions.iter_mut().find(|region| region.id == id))
        else {
            return false;
        };
        if region.name == name {
            return false;
        }
        region.name = name;
        true
    }

    pub fn remove_region(&mut self, id: RegionId) -> bool {
        let Some(audio) = self.active_audio_mut() else {
            return false;
        };
        let before = audio.regions.len();
        audio.regions.retain(|region| region.id != id);
        audio.regions.len() != before
    }

    pub fn auto_detect_regions(&mut self) -> usize {
        let Some(audio) = self.active_audio() else {
            return 0;
        };
        let intervals = detect_regions(
            &audio.waveform,
            audio.sample_rate,
            audio.duration_ms(),
            AUTO_THRESHOLD,
            AUTO_SILENCE_MS,
            AUTO_PADDING_MS,
        );
        if let Some(audio) = self.active_audio_mut() {
            audio.regions.clear();
        }
        for (start, end) in intervals {
            let _ = self.add_region(start, end);
        }
        self.active_audio().map_or(0, |audio| audio.regions.len())
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        crate::project_archive::save_voicelines_file(path, self).map_err(|error| error.to_string())
    }

    pub fn load(path: &Path) -> Result<crate::project_archive::LoadedVoicelinesProject, String> {
        crate::project_archive::load_voicelines_file(path).map_err(|error| error.to_string())
    }

    fn active_audio_mut(&mut self) -> Option<&mut Audio> {
        let id = self.active_audio?;
        self.audios.iter_mut().find(|audio| audio.id == id)
    }

    fn allocate_id(&mut self) -> u64 {
        let id = self.next_id.max(1);
        self.next_id = id.saturating_add(1);
        id
    }

    fn allocate_region_name(&mut self) -> String {
        match &mut self.naming {
            NamingMode::Manual => "Sans nom".into(),
            NamingMode::Automatic { pattern, next } => {
                let name = pattern.replace("{num}", &format!("{:03}", *next));
                *next = next.saturating_add(1);
                name
            }
        }
    }

    fn next_automatic_number(&self) -> u32 {
        self.audios
            .iter()
            .map(|audio| audio.regions.len() as u32)
            .sum::<u32>()
            .saturating_add(1)
    }

    pub(crate) fn validate(&mut self) -> Result<(), String> {
        match &self.naming {
            NamingMode::Automatic { pattern, .. } if !pattern.contains("{num}") => {
                return Err("invalid voicelines naming pattern".into());
            }
            NamingMode::Automatic { pattern, .. } => self.automatic_pattern.clone_from(pattern),
            NamingMode::Manual if !self.automatic_pattern.contains("{num}") => {
                self.automatic_pattern = default_automatic_pattern();
            }
            NamingMode::Manual => {}
        }
        let mut ids = HashSet::new();
        for audio in &mut self.audios {
            if audio.sample_rate == 0 || audio.sample_count == 0 || !ids.insert(audio.id) {
                return Err("invalid voicelines audio metadata".into());
            }
            let duration = audio.duration_ms();
            for region in &audio.regions {
                if !ids.insert(region.id)
                    || valid_bounds(region.start_ms, region.end_ms, duration).is_none()
                    || clean_name(&region.name).is_empty()
                {
                    return Err("invalid voicelines region".into());
                }
            }
            audio
                .regions
                .sort_by_key(|region| (region.start_ms, region.id));
        }
        if self
            .active_audio
            .is_some_and(|id| !self.audios.iter().any(|audio| audio.id == id))
        {
            self.active_audio = self.audios.first().map(|audio| audio.id);
        }
        self.next_id = self
            .next_id
            .max(ids.into_iter().max().unwrap_or(0).saturating_add(1));
        Ok(())
    }
}

pub fn export_region(audio: &Audio, region: &Region, output: &Path) -> Result<(), String> {
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create voicelines export directory: {error}"))?;
    }
    let duration = (region.end_ms - region.start_ms) as f64 / 1_000.0;
    let mut command = crate::media_binary::command("ffmpeg");
    command
        .args(["-y", "-v", "error", "-ss"])
        .arg(format!("{:.3}", region.start_ms as f64 / 1_000.0))
        .arg("-i")
        .arg(&audio.playback_path)
        .args(["-t", &format!("{duration:.3}"), "-vn", "-c:a"]);
    if output
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("flac"))
    {
        command.arg("flac");
    } else {
        command.args(["libvorbis", "-q:a", "6"]);
    }
    let status = command
        .arg(output)
        .status()
        .map_err(|error| format!("cannot start FFmpeg voiceline export: {error}"))?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| format!("FFmpeg could not export {}", region.name))
}

pub fn export_all(audio: &Audio, directory: &Path) -> Result<Vec<PathBuf>, String> {
    fs::create_dir_all(directory)
        .map_err(|error| format!("cannot create voicelines export directory: {error}"))?;
    let mut reserved = HashSet::new();
    let mut outputs = Vec::with_capacity(audio.regions.len());
    for region in &audio.regions {
        let stem = export_stem(&region.name);
        let mut suffix = 1;
        let output = loop {
            let name = if suffix == 1 {
                format!("{stem}.ogg")
            } else {
                format!("{stem}_{suffix}.ogg")
            };
            let candidate = directory.join(name);
            if reserved.insert(candidate.clone()) && !candidate.exists() {
                break candidate;
            }
            suffix += 1;
        };
        export_region(audio, region, &output)?;
        outputs.push(output);
    }
    Ok(outputs)
}

pub fn export_stem(name: &str) -> String {
    let stem = name
        .trim()
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
        .collect::<String>();
    let stem = stem.trim_matches([' ', '.']).trim();
    if stem.is_empty() {
        "voiceline".into()
    } else if is_windows_device_name(stem) {
        format!("_{stem}")
    } else {
        stem.into()
    }
}

fn is_windows_device_name(name: &str) -> bool {
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ["COM", "LPT"].iter().any(|prefix| {
            stem.strip_prefix(prefix).is_some_and(|suffix| {
                matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
            })
        })
}

fn clean_name(name: &str) -> String {
    name.trim()
        .chars()
        .filter(|character| !character.is_control())
        .take(120)
        .collect()
}

fn valid_bounds(start_ms: u64, end_ms: u64, duration_ms: u64) -> Option<(u64, u64)> {
    let start = start_ms.min(duration_ms);
    let end = end_ms.min(duration_ms);
    (end >= start.saturating_add(MIN_REGION_MS)).then_some((start, end))
}

pub fn detect_regions(
    waveform: &WaveformData,
    sample_rate: u32,
    duration_ms: u64,
    threshold: f32,
    min_silence_ms: u64,
    padding_ms: u64,
) -> Vec<(u64, u64)> {
    if waveform.peaks.is_empty() || sample_rate == 0 || duration_ms < MIN_REGION_MS {
        return Vec::new();
    }
    let peak_ms = f64::from(waveform.samples_per_peak.max(1)) * 1_000.0 / f64::from(sample_rate);
    let min_silence = ((min_silence_ms as f64 / peak_ms).ceil() as usize).max(1);
    let mut result = Vec::new();
    let mut start = None;
    let mut silent = 0usize;

    for (index, peak) in waveform.peaks.iter().copied().enumerate() {
        if peak >= threshold {
            start.get_or_insert(index);
            silent = 0;
        } else if start.is_some() {
            silent += 1;
            if silent >= min_silence {
                let end = index + 1 - silent;
                push_detected_region(
                    &mut result,
                    start.take().unwrap(),
                    end,
                    peak_ms,
                    duration_ms,
                    padding_ms,
                );
                silent = 0;
            }
        }
    }
    if let Some(start) = start {
        let end = waveform.peaks.len().saturating_sub(silent);
        push_detected_region(&mut result, start, end, peak_ms, duration_ms, padding_ms);
    }
    result
}

fn push_detected_region(
    result: &mut Vec<(u64, u64)>,
    start_peak: usize,
    end_peak: usize,
    peak_ms: f64,
    duration_ms: u64,
    padding_ms: u64,
) {
    let start = (start_peak as f64 * peak_ms).round() as u64;
    let end = (end_peak as f64 * peak_ms).round() as u64;
    let bounds = (
        start.saturating_sub(padding_ms),
        end.saturating_add(padding_ms).min(duration_ms),
    );
    if bounds.1 >= bounds.0.saturating_add(MIN_REGION_MS) {
        result.push(bounds);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recorded(peaks: Vec<f32>) -> RecordedAudio {
        RecordedAudio {
            file_name: "audio.flac".into(),
            sample_rate: 1_000,
            channels: 1,
            sample_count: peaks.len() as u64 * 100,
            checksum: "a".repeat(40),
            waveform: WaveformData::new(100, peaks).unwrap(),
        }
    }

    #[test]
    fn several_audio_files_keep_independent_regions_and_selection() {
        let mut project = VoicelinesProject::default();
        let first = project.add_audio("one.wav".into(), "one.flac".into(), recorded(vec![1.0; 20]));
        let first_region = project.add_region(100, 500).unwrap();
        let second =
            project.add_audio("two.wav".into(), "two.flac".into(), recorded(vec![1.0; 30]));
        project.add_region(200, 800).unwrap();

        assert_eq!(project.active_audio_id(), Some(second));
        assert!(project.select_audio(first));
        assert_eq!(project.active_audio().unwrap().regions[0].id, first_region);
        assert_eq!(project.audio(second).unwrap().regions.len(), 1);
    }

    #[test]
    fn automatic_names_are_stable_and_manual_names_are_cleaned() {
        let mut project = VoicelinesProject::default();
        project.add_audio("one.wav".into(), "one.flac".into(), recorded(vec![1.0; 20]));
        let first = project.add_region(0, 100).unwrap();
        assert_eq!(
            project.active_audio().unwrap().regions[0].name,
            "voiceline_001"
        );
        project.set_manual_naming();
        let second = project.add_region(200, 300).unwrap();
        assert!(project.rename_region(second, "  bonjour\n  "));
        assert_eq!(project.active_audio().unwrap().regions[1].name, "bonjour");
        assert!(project.remove_region(first));
    }

    #[test]
    fn silence_detection_joins_short_gaps_and_splits_long_ones() {
        let waveform = WaveformData::new(
            100,
            vec![0.0, 0.2, 0.2, 0.0, 0.2, 0.2, 0.0, 0.0, 0.0, 0.3, 0.3, 0.0],
        )
        .unwrap();
        assert_eq!(
            detect_regions(&waveform, 1_000, 1_200, 0.01, 200, 0),
            vec![(100, 600), (900, 1_100)]
        );
    }

    #[test]
    fn export_names_are_windows_safe() {
        assert_eq!(export_stem("  salut:toi?.  "), "salut_toi_");
        assert_eq!(export_stem("..."), "voiceline");
        assert_eq!(export_stem("CON"), "_CON");
    }

    #[test]
    fn session_round_trip_keeps_sources_regions_and_naming() {
        let path = std::env::temp_dir().join(format!(
            "coquerythmo-voicelines-{}.coquerythmo",
            std::process::id()
        ));
        let audio_path = std::env::temp_dir().join(format!(
            "coquerythmo-voicelines-audio-{}.flac",
            std::process::id()
        ));
        std::fs::write(&audio_path, b"embedded audio").unwrap();
        let mut project = VoicelinesProject::default();
        project.add_audio(
            "dialogue.wav".into(),
            audio_path.clone(),
            recorded(vec![0.0, 1.0, 0.0]),
        );
        project.add_region(20, 200).unwrap();
        project.set_manual_naming();

        project.save(&path).unwrap();
        let loaded = VoicelinesProject::load(&path).unwrap();
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(audio_path);

        assert_eq!(
            loaded.project.audios()[0].source_path,
            PathBuf::from("dialogue.wav")
        );
        assert_eq!(
            std::fs::read(&loaded.audio_paths[&loaded.project.audios()[0].id]).unwrap(),
            b"embedded audio"
        );
        assert_eq!(loaded.project.audios()[0].regions[0].name, "voiceline_001");
        assert_eq!(loaded.project.naming(), &NamingMode::Manual);
    }

    #[test]
    fn automatic_naming_accepts_a_plain_base_name_or_a_pattern() {
        let mut project = VoicelinesProject::default();
        project.add_audio("one.wav".into(), "one.flac".into(), recorded(vec![1.0; 20]));
        assert!(project.set_automatic_naming("réplique-{num}").is_ok());
        project.add_region(0, 100).unwrap();
        assert_eq!(
            project.active_audio().unwrap().regions[0].name,
            "réplique-001"
        );
        assert!(project.set_automatic_naming("autre").is_ok());
        project.add_region(200, 300).unwrap();
        assert_eq!(project.active_audio().unwrap().regions[1].name, "autre_002");
        project.set_manual_naming();
        assert_eq!(project.automatic_pattern(), "autre_{num}");
        assert!(project.set_automatic_naming("  ").is_err());

        project.set_automatic_naming("archive-{num}").unwrap();
        let mut old_json = serde_json::to_value(&project).unwrap();
        old_json
            .as_object_mut()
            .unwrap()
            .remove("automatic_pattern");
        let mut loaded: VoicelinesProject = serde_json::from_value(old_json).unwrap();
        loaded.validate().unwrap();
        assert_eq!(loaded.automatic_pattern(), "archive-{num}");
    }
}
