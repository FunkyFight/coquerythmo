//! Préparation d'un audio et passage d'un modèle, en arrière-plan.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use super::install::{for_each_line, python_command, write_engine_script};
use super::{InstallLocation, ModelSpec, ResultGroup, ResultStem, Track, MODEL_SAMPLE_RATE};

static OUTPUT_NONCE: AtomicU64 = AtomicU64::new(0);

/// Convertit n'importe quel audio (ou la piste son d'une vidéo) au format des
/// modèles et en calcule la forme d'onde.
pub fn prepare_source(input: &Path, directory: &Path) -> Result<Track, String> {
    fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    let output = directory.join("source.wav");
    let mut command = crate::media_binary::command("ffmpeg");
    command
        .args(["-y", "-v", "error", "-i"])
        .arg(input)
        .args(["-vn", "-map", "0:a:0", "-ar"])
        .arg(MODEL_SAMPLE_RATE.to_string())
        .args(["-ac", "2", "-c:a", "pcm_s24le", "-rf64", "auto"])
        .arg(&output)
        .stdin(Stdio::null());
    super::hide_console(&mut command);
    let result = command.output().map_err(|error| error.to_string())?;
    if !result.status.success() {
        let _ = fs::remove_file(&output);
        let detail = String::from_utf8_lossy(&result.stderr);
        return Err(detail
            .lines()
            .last()
            .unwrap_or("FFmpeg could not read this file")
            .to_owned());
    }
    analyse(&output)
}

/// Forme d'onde et durée d'un fichier audio. Passe par une copie mono
/// temporaire, la même analyse que les autres onglets.
pub fn analyse(path: &Path) -> Result<Track, String> {
    let proxy = path.with_extension("analysis.flac");
    let recorded =
        crate::media_recording::import_audio(path, &proxy).map_err(|error| error.to_string());
    let _ = fs::remove_file(&proxy);
    let recorded = recorded?;
    let sample_rate = recorded.sample_rate.max(1);
    Ok(Track {
        path: path.to_path_buf(),
        duration_ms: recorded.sample_count.saturating_mul(1_000) / u64::from(sample_rate),
        waveform: Arc::new(recorded.waveform),
        waveform_sample_rate: sample_rate,
    })
}

/// Écrit une piste dans le format que donne l'extension choisie.
pub fn export_track(source: &Path, destination: &Path) -> Result<(), String> {
    let extension = destination
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    let codec: &[&str] = match extension.as_str() {
        "wav" => &["-c:a", "pcm_s24le"],
        "flac" => &["-c:a", "flac"],
        "mp3" => &["-c:a", "libmp3lame", "-b:a", "320k"],
        "ogg" => &["-c:a", "libvorbis", "-q:a", "8"],
        "m4a" => &["-c:a", "aac", "-b:a", "256k"],
        _ => return Err(format!(".{extension}")),
    };
    let mut command = crate::media_binary::command("ffmpeg");
    command
        .args(["-y", "-v", "error", "-i"])
        .arg(source)
        .args(codec)
        .arg(destination)
        .stdin(Stdio::null());
    super::hide_console(&mut command);
    let result = command.output().map_err(|error| error.to_string())?;
    if result.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&result.stderr)
            .lines()
            .last()
            .unwrap_or("FFmpeg")
            .to_owned())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Loading,
    Separating,
    Finishing,
}

/// Ce que le moteur a dit de la machine au début du traitement.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DeviceReport {
    pub cuda: bool,
    pub gpu: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JobProgress {
    pub stage: Stage,
    /// Part de l'audio déjà traitée, de 0 à 1.
    pub fraction: f32,
    /// Secondes d'audio traitées par seconde écoulée.
    pub speed: Option<f32>,
    pub remaining_seconds: Option<f32>,
    pub device: Option<DeviceReport>,
    /// Le moteur a réduit ses réglages faute de mémoire vidéo.
    pub reduced: bool,
}

#[derive(Debug)]
pub enum JobEnd {
    Done(ResultGroup),
    Cancelled,
    Failed(String),
}

struct Shared {
    progress: Mutex<JobProgress>,
    end: Mutex<Option<JobEnd>>,
    cancel: AtomicBool,
    child: Mutex<Option<Child>>,
}

pub struct JobHandle {
    pub item_id: u64,
    /// Piste de l'audio qui est traitée : les résultats en descendront.
    pub parent: u64,
    pub model: &'static ModelSpec,
    shared: Arc<Shared>,
}

impl JobHandle {
    pub fn progress(&self) -> JobProgress {
        self.shared
            .progress
            .lock()
            .map(|progress| progress.clone())
            .unwrap_or(JobProgress {
                stage: Stage::Loading,
                fraction: 0.0,
                speed: None,
                remaining_seconds: None,
                device: None,
                reduced: false,
            })
    }

    /// Le résultat, une seule fois, quand le traitement est fini.
    pub fn take_end(&self) -> Option<JobEnd> {
        self.shared.end.lock().ok().and_then(|mut end| end.take())
    }

    pub fn cancel(&self) {
        self.shared.cancel.store(true, Ordering::Relaxed);
        if let Ok(mut child) = self.shared.child.lock() {
            if let Some(child) = child.as_mut() {
                let _ = child.kill();
            }
        }
    }
}

/// Passe `model` sur `source` (déjà au format des modèles). Les pistes vont
/// dans un nouveau sous-dossier de `item_directory`.
pub fn start(
    location: InstallLocation,
    item_id: u64,
    parent: u64,
    model: &'static ModelSpec,
    source: PathBuf,
    item_directory: PathBuf,
    best_quality: bool,
) -> JobHandle {
    let shared = Arc::new(Shared {
        progress: Mutex::new(JobProgress {
            stage: Stage::Loading,
            fraction: 0.0,
            speed: None,
            remaining_seconds: None,
            device: None,
            reduced: false,
        }),
        end: Mutex::new(None),
        cancel: AtomicBool::new(false),
        child: Mutex::new(None),
    });
    let worker = shared.clone();
    std::thread::spawn(move || {
        let nonce = OUTPUT_NONCE.fetch_add(1, Ordering::Relaxed);
        let directory = item_directory.join(format!("{}-{}-{nonce}", model.id, std::process::id()));
        let result = run(&worker, &location, model, &source, &directory, best_quality);
        let end = match result {
            _ if worker.cancel.load(Ordering::Relaxed) => JobEnd::Cancelled,
            Ok(stems) => JobEnd::Done(ResultGroup {
                model,
                directory: directory.clone(),
                stems,
            }),
            Err(error) => JobEnd::Failed(error),
        };
        if !matches!(end, JobEnd::Done(_)) {
            let _ = fs::remove_dir_all(&directory);
        }
        if let Ok(mut slot) = worker.end.lock() {
            *slot = Some(end);
        }
    });
    JobHandle {
        item_id,
        parent,
        model,
        shared,
    }
}

fn run(
    shared: &Shared,
    location: &InstallLocation,
    model: &'static ModelSpec,
    source: &Path,
    directory: &Path,
    best_quality: bool,
) -> Result<Vec<ResultStem>, String> {
    let script = write_engine_script(location)?;
    fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    let mut command = python_command(location);
    command
        .arg(&script)
        .arg("separate")
        .arg("--model")
        .arg(location.checkpoint_path(model))
        .arg("--config")
        .arg(location.config_path(model))
        .arg("--input")
        .arg(source)
        .arg("--output-dir")
        .arg(directory)
        .args(["--overlap", if best_quality { "4" } else { "2" }])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|error| error.to_string())?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    if let Ok(mut slot) = shared.child.lock() {
        *slot = Some(child);
    }
    let error_tail = Arc::new(Mutex::new(Vec::<String>::new()));
    let error_lines = error_tail.clone();
    let error_reader = std::thread::spawn(move || {
        if let Some(stderr) = stderr {
            for_each_line(stderr, |line| {
                if let Ok(mut lines) = error_lines.lock() {
                    if lines.len() == 8 {
                        lines.remove(0);
                    }
                    lines.push(line.to_owned());
                }
            });
        }
    });

    let mut stems = Vec::new();
    let mut failure = None;
    let mut finished = false;
    if let Some(stdout) = stdout {
        for_each_line(stdout, |line| {
            let Some(event) = parse_event(line) else {
                return;
            };
            match event {
                EngineEvent::Stage(stage) => update(shared, |progress| progress.stage = stage),
                EngineEvent::Device(device) => {
                    update(shared, |progress| progress.device = Some(device))
                }
                EngineEvent::Notice => update(shared, |progress| progress.reduced = true),
                EngineEvent::Progress {
                    done,
                    total,
                    elapsed,
                } => update(shared, |progress| {
                    apply_progress(progress, done, total, elapsed)
                }),
                EngineEvent::Stem { name, path } => stems.push((name, path)),
                EngineEvent::Done => finished = true,
                EngineEvent::Error(message) => failure = Some(message),
            }
        });
    }
    let _ = error_reader.join();
    let status = shared
        .child
        .lock()
        .ok()
        .and_then(|mut slot| slot.take())
        .map(|mut child| child.wait());
    if shared.cancel.load(Ordering::Relaxed) {
        return Err("cancelled".into());
    }
    if let Some(message) = failure {
        return Err(message);
    }
    if !finished || !matches!(status, Some(Ok(status)) if status.success()) {
        return Err(error_tail
            .lock()
            .ok()
            .map(|lines| lines.join("\n"))
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| "the engine stopped unexpectedly".into()));
    }

    update(shared, |progress| {
        progress.stage = Stage::Finishing;
        progress.fraction = 1.0;
    });
    stems
        .into_iter()
        .map(|(name, path)| {
            Ok(ResultStem {
                label_key: model.stem_label_key(&name),
                track: analyse(&path)?,
                name,
            })
        })
        .collect()
}

fn update(shared: &Shared, change: impl FnOnce(&mut JobProgress)) {
    if let Ok(mut progress) = shared.progress.lock() {
        change(&mut progress);
    }
}

fn apply_progress(progress: &mut JobProgress, done: u64, total: u64, elapsed: f32) {
    progress.stage = Stage::Separating;
    progress.fraction = if total == 0 {
        0.0
    } else {
        (done as f32 / total as f32).clamp(0.0, 1.0)
    };
    if elapsed > 0.5 && done > 0 {
        let rate = MODEL_SAMPLE_RATE as f32;
        let speed = done as f32 / rate / elapsed;
        progress.speed = Some(speed);
        progress.remaining_seconds =
            Some((total.saturating_sub(done) as f32 / rate / speed.max(0.001)).max(0.0));
    }
}

#[derive(Debug, PartialEq)]
enum EngineEvent {
    Stage(Stage),
    Device(DeviceReport),
    Notice,
    Progress { done: u64, total: u64, elapsed: f32 },
    Stem { name: String, path: PathBuf },
    Done,
    Error(String),
}

/// Une ligne JSON du moteur. Tout le reste (avertissements de bibliothèques)
/// est ignoré.
fn parse_event(line: &str) -> Option<EngineEvent> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    let text = |key: &str| value.get(key).and_then(|field| field.as_str());
    Some(match text("event")? {
        "stage" => EngineEvent::Stage(match text("name")? {
            "loading" => Stage::Loading,
            "separating" => Stage::Separating,
            _ => return None,
        }),
        "device" => EngineEvent::Device(DeviceReport {
            cuda: value.get("cuda").and_then(|field| field.as_bool()) == Some(true),
            gpu: text("gpu").map(str::to_owned),
        }),
        "notice" => EngineEvent::Notice,
        "progress" => EngineEvent::Progress {
            done: value.get("done")?.as_u64()?,
            total: value.get("total")?.as_u64()?,
            elapsed: value.get("elapsed").and_then(|field| field.as_f64())? as f32,
        },
        "stem" => EngineEvent::Stem {
            name: text("name")?.to_owned(),
            path: PathBuf::from(text("path")?),
        },
        "done" => EngineEvent::Done,
        "error" => EngineEvent::Error(text("message").unwrap_or("error").to_owned()),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_lines_become_events_and_noise_is_ignored() {
        assert_eq!(
            parse_event(r#"{"name": "separating", "batch": 1, "event": "stage"}"#),
            Some(EngineEvent::Stage(Stage::Separating))
        );
        assert_eq!(
            parse_event(
                r#"{"done": 441000, "total": 882000, "elapsed": 2.0, "event": "progress"}"#
            ),
            Some(EngineEvent::Progress {
                done: 441_000,
                total: 882_000,
                elapsed: 2.0
            })
        );
        assert_eq!(
            parse_event(
                r#"{"name": "Vocals", "path": "C:\\out\\01_Vocals.flac", "event": "stem"}"#
            ),
            Some(EngineEvent::Stem {
                name: "Vocals".into(),
                path: PathBuf::from(r"C:\out\01_Vocals.flac"),
            })
        );
        assert_eq!(
            parse_event(
                r#"{"torch": "2.8.0+cu128", "cuda": true, "gpu": "RTX 4060", "event": "device"}"#
            ),
            Some(EngineEvent::Device(DeviceReport {
                cuda: true,
                gpu: Some("RTX 4060".into()),
            }))
        );
        assert_eq!(parse_event("UserWarning: something"), None);
        assert_eq!(parse_event(r#"{"event": "unknown"}"#), None);
    }

    #[test]
    fn progress_reports_speed_and_remaining_time() {
        let mut progress = JobProgress {
            stage: Stage::Loading,
            fraction: 0.0,
            speed: None,
            remaining_seconds: None,
            device: None,
            reduced: false,
        };
        // 20 s d'audio traitées en 2 s sur 60 s : x10, il reste 4 s.
        apply_progress(&mut progress, 882_000, 2_646_000, 2.0);
        assert_eq!(progress.stage, Stage::Separating);
        assert!((progress.fraction - 1.0 / 3.0).abs() < 1e-4);
        assert!((progress.speed.unwrap() - 10.0).abs() < 1e-3);
        assert!((progress.remaining_seconds.unwrap() - 4.0).abs() < 1e-3);
    }

    #[test]
    fn export_refuses_an_unknown_format_before_running_ffmpeg() {
        let error = export_track(Path::new("in.flac"), Path::new("out.xyz")).unwrap_err();
        assert_eq!(error, ".xyz");
    }
}
