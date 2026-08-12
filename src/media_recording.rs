//! Native microphone capture adapter for the backend-neutral recording model.
//!
//! CPAL owns the real-time input callback. The callback only converts samples
//! and sends them asynchronously; a worker thread streams raw PCM
//! to FFmpeg and builds the waveform/checksum. No audio hardware is needed to
//! test the pure helpers at the bottom of this module.

use crate::integrity::Sha1;
use crate::recording::{AudioRecorder, RecordedAudio, RecordingError, WaveformData};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SizedSample};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, RwLock};
use std::thread::{self, JoinHandle};

const WAVEFORM_PEAKS_PER_SECOND: u32 = 100;
const MIN_RECORDING_SAMPLE_RATE: u32 = 48_000;

static TEMP_FILE_NONCE: AtomicU64 = AtomicU64::new(0);

pub fn import_audio(source: &Path, output: &Path) -> Result<RecordedAudio, RecordingError> {
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            recorder_error(format!("cannot create audio import directory: {error}"))
        })?;
    }
    let status = crate::media_binary::command("ffmpeg")
        .args(["-y", "-v", "error", "-i"])
        .arg(source)
        .args(["-vn", "-ar", "48000", "-ac", "1", "-c:a", "flac"])
        .arg(output)
        .status()
        .map_err(|error| recorder_error(format!("cannot import audio: {error}")))?;
    if !status.success() {
        remove_if_present(output);
        return Err(recorder_error("FFmpeg could not import this audio file"));
    }

    let samples = crate::recording_mix::decode_realtime_asset(
        output,
        &std::sync::atomic::AtomicBool::new(false),
    )
    .map_err(recorder_error)?;
    if samples.is_empty() {
        remove_if_present(output);
        return Err(recorder_error("imported audio is empty"));
    }
    let mut waveform = WaveformAccumulator::new(1, 480);
    waveform.push_interleaved(&samples);
    Ok(RecordedAudio {
        file_name: output
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("import.flac")
            .to_owned(),
        sample_rate: 48_000,
        channels: 1,
        sample_count: samples.len() as u64,
        checksum: sha1_file(output)?,
        waveform: waveform.finish(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputDeviceIssue {
    DefaultConfigUnavailable,
    SupportedConfigUnavailable,
    SampleRateTooLow(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputDeviceInfo {
    pub name: String,
    pub issue: Option<InputDeviceIssue>,
}

/// Records one microphone take to a lossless FLAC file.
///
/// Construct a fresh adapter (or call `set_output_path`) for each take. The
/// final path is never overwritten: an existing file makes `start` fail.
pub struct FfmpegFlacRecorder {
    output_path: PathBuf,
    input_device: Option<String>,
    active: Option<ActiveRecording>,
}

struct ActiveRecording {
    stream: cpal::Stream,
    worker: JoinHandle<Result<WorkerSummary, RecordingError>>,
    temporary_path: PathBuf,
    live_waveform: Arc<RwLock<WaveformData>>,
    stream_error: Arc<Mutex<Option<String>>>,
}

#[derive(Debug)]
struct WorkerSummary {
    sample_rate: u32,
    channels: u16,
    sample_count: u64,
    checksum: String,
    waveform: WaveformData,
}

impl FfmpegFlacRecorder {
    pub fn new(output_path: impl Into<PathBuf>) -> Self {
        Self {
            output_path: output_path.into(),
            input_device: None,
            active: None,
        }
    }

    pub fn with_input_device(mut self, input_device: Option<String>) -> Self {
        self.input_device = input_device;
        self
    }

    pub fn output_path(&self) -> &Path {
        &self.output_path
    }

    pub fn set_output_path(
        &mut self,
        output_path: impl Into<PathBuf>,
    ) -> Result<(), RecordingError> {
        if self.active.is_some() {
            return Err(RecordingError::CaptureBusy);
        }
        self.output_path = output_path.into();
        Ok(())
    }

    fn clean_failed_start(
        worker: JoinHandle<Result<WorkerSummary, RecordingError>>,
        temporary_path: &Path,
    ) {
        let _ = worker.join();
        remove_if_present(temporary_path);
    }
}

impl AudioRecorder for FfmpegFlacRecorder {
    fn start(&mut self) -> Result<(), RecordingError> {
        if self.active.is_some() {
            return Err(RecordingError::CaptureBusy);
        }
        if self.output_path.as_os_str().is_empty() {
            return Err(recorder_error("recording output path is empty"));
        }
        if !self
            .output_path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("flac"))
        {
            return Err(recorder_error(
                "recording output must use the .flac extension",
            ));
        }
        if self.output_path.exists() {
            return Err(recorder_error(format!(
                "recording output already exists: {}",
                self.output_path.display()
            )));
        }
        if let Some(parent) = self
            .output_path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).map_err(|error| {
                recorder_error(format!(
                    "cannot create recording directory {}: {error}",
                    parent.display()
                ))
            })?;
        }

        let host = cpal::default_host();
        let device = input_device(&host, self.input_device.as_deref())?;
        let supported_config = recording_input_config(&device)?;
        let sample_format = supported_config.sample_format();
        let stream_config: cpal::StreamConfig = supported_config.into();
        let sample_rate = stream_config.sample_rate.0;
        let channels = stream_config.channels;
        if sample_rate == 0 || channels == 0 {
            return Err(recorder_error("audio input configuration is empty"));
        }

        let temporary_path = temporary_path_for(
            &self.output_path,
            TEMP_FILE_NONCE.fetch_add(1, Ordering::Relaxed),
        );
        remove_if_present(&temporary_path);

        let samples_per_peak = (sample_rate / WAVEFORM_PEAKS_PER_SECOND).max(1);
        let live_waveform = Arc::new(RwLock::new(WaveformData {
            samples_per_peak,
            peaks: Vec::new(),
        }));
        let stream_error = Arc::new(Mutex::new(None));
        // ponytail: keep the real-time callback lossless; spool PCM if sustained encoder lag
        // is ever observed in production.
        let (sender, receiver) = mpsc::channel();

        let worker_path = temporary_path.clone();
        let worker_waveform = Arc::clone(&live_waveform);
        let worker = thread::Builder::new()
            .name("recording-flac-writer".to_owned())
            .spawn(move || {
                run_flac_worker(
                    receiver,
                    &worker_path,
                    sample_rate,
                    channels,
                    samples_per_peak,
                    worker_waveform,
                )
            })
            .map_err(|error| recorder_error(format!("cannot start recording worker: {error}")))?;

        let stream = match build_input_stream(
            &device,
            &stream_config,
            sample_format,
            sender,
            Arc::clone(&stream_error),
        ) {
            Ok(stream) => stream,
            Err(error) => {
                Self::clean_failed_start(worker, &temporary_path);
                return Err(error);
            }
        };

        if let Err(error) = stream.play() {
            drop(stream);
            Self::clean_failed_start(worker, &temporary_path);
            return Err(recorder_error(format!(
                "cannot start audio input stream: {error}"
            )));
        }

        self.active = Some(ActiveRecording {
            stream,
            worker,
            temporary_path,
            live_waveform,
            stream_error,
        });
        Ok(())
    }

    fn stop(&mut self) -> Result<RecordedAudio, RecordingError> {
        let active = self.active.take().ok_or(RecordingError::CaptureNotActive)?;
        let ActiveRecording {
            stream,
            worker,
            temporary_path,
            live_waveform: _,
            stream_error,
        } = active;

        // Dropping the CPAL stream drops its callback and the final channel
        // sender. The worker can then drain queued samples and close FFmpeg.
        drop(stream);
        let worker_result = worker
            .join()
            .map_err(|_| recorder_error("recording worker panicked"))?;

        let summary = match worker_result {
            Ok(summary) => summary,
            Err(error) => {
                remove_if_present(&temporary_path);
                return Err(error);
            }
        };
        if let Some(error) = stream_error.lock().ok().and_then(|mut error| error.take()) {
            remove_if_present(&temporary_path);
            return Err(recorder_error(format!(
                "audio input stream failed: {error}"
            )));
        }
        if summary.sample_count == 0 {
            remove_if_present(&temporary_path);
            return Err(recorder_error("recording contains no audio samples"));
        }

        fs::rename(&temporary_path, &self.output_path).map_err(|error| {
            remove_if_present(&temporary_path);
            recorder_error(format!(
                "cannot finalize recording {}: {error}",
                self.output_path.display()
            ))
        })?;

        let file_name = self
            .output_path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .ok_or_else(|| recorder_error("recording output path has no file name"))?;
        Ok(RecordedAudio {
            file_name,
            sample_rate: summary.sample_rate,
            channels: summary.channels,
            sample_count: summary.sample_count,
            checksum: summary.checksum,
            waveform: summary.waveform,
        })
    }

    fn is_recording(&self) -> bool {
        self.active.is_some()
    }

    fn live_waveform(&self) -> WaveformData {
        self.active
            .as_ref()
            .and_then(|active| {
                active
                    .live_waveform
                    .read()
                    .ok()
                    .map(|waveform| waveform.clone())
            })
            .unwrap_or_default()
    }
}

impl Drop for FfmpegFlacRecorder {
    fn drop(&mut self) {
        let Some(active) = self.active.take() else {
            return;
        };
        let ActiveRecording {
            stream,
            worker,
            temporary_path,
            ..
        } = active;
        drop(stream);
        let _ = worker.join();
        remove_if_present(&temporary_path);
    }
}

fn build_input_stream(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    sample_format: cpal::SampleFormat,
    sender: Sender<Vec<f32>>,
    stream_error: Arc<Mutex<Option<String>>>,
) -> Result<cpal::Stream, RecordingError> {
    match sample_format {
        cpal::SampleFormat::I8 => {
            build_typed_input_stream::<i8>(device, config, sender, stream_error)
        }
        cpal::SampleFormat::F32 => {
            build_typed_input_stream::<f32>(device, config, sender, stream_error)
        }
        cpal::SampleFormat::I16 => {
            build_typed_input_stream::<i16>(device, config, sender, stream_error)
        }
        cpal::SampleFormat::I32 => {
            build_typed_input_stream::<i32>(device, config, sender, stream_error)
        }
        cpal::SampleFormat::I64 => {
            build_typed_input_stream::<i64>(device, config, sender, stream_error)
        }
        cpal::SampleFormat::U8 => {
            build_typed_input_stream::<u8>(device, config, sender, stream_error)
        }
        cpal::SampleFormat::U16 => {
            build_typed_input_stream::<u16>(device, config, sender, stream_error)
        }
        cpal::SampleFormat::U32 => {
            build_typed_input_stream::<u32>(device, config, sender, stream_error)
        }
        cpal::SampleFormat::U64 => {
            build_typed_input_stream::<u64>(device, config, sender, stream_error)
        }
        cpal::SampleFormat::F64 => {
            build_typed_input_stream::<f64>(device, config, sender, stream_error)
        }
        other => Err(recorder_error(format!(
            "unsupported audio input sample format: {other:?}"
        ))),
    }
}

fn build_typed_input_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    sender: Sender<Vec<f32>>,
    stream_error: Arc<Mutex<Option<String>>>,
) -> Result<cpal::Stream, RecordingError>
where
    T: Sample + SizedSample + Copy,
    f32: FromSample<T>,
{
    let error_slot = Arc::clone(&stream_error);
    device
        .build_input_stream(
            config,
            move |input: &[T], _info: &cpal::InputCallbackInfo| {
                if input.is_empty() {
                    return;
                }
                // Format conversion only: no gain, normalization, compression or filtering.
                let samples = input_samples_as_f32(input);
                if sender.send(samples).is_err() {
                    store_first_error(&stream_error, "recording worker disconnected".to_owned());
                }
            },
            move |error| store_first_error(&error_slot, error.to_string()),
            None,
        )
        .map_err(|error| recorder_error(format!("cannot build audio input stream: {error}")))
}

pub fn input_device_names() -> Result<Vec<InputDeviceInfo>, RecordingError> {
    let mut devices = cpal::default_host()
        .input_devices()
        .map_err(|error| recorder_error(format!("cannot enumerate audio input devices: {error}")))?
        .filter_map(|device| {
            let name = device.name().ok()?;
            Some(inspect_input_device(device, name))
        })
        .collect::<Vec<_>>();
    devices.sort_by(|left, right| left.name.cmp(&right.name));
    devices.dedup_by(|left, right| left.name == right.name);
    Ok(devices)
}

/// Opens and starts the selected input once so OS privacy prompts and device
/// failures happen before a synchronized online countdown.
pub fn preflight_input_device(selected_name: Option<&str>) -> Result<(), RecordingError> {
    let host = cpal::default_host();
    let device = input_device(&host, selected_name)?;
    let supported = recording_input_config(&device)?;
    let sample_format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let (sender, _receiver) = mpsc::channel();
    let stream_error = Arc::new(Mutex::new(None));
    let stream = build_input_stream(&device, &config, sample_format, sender, stream_error)?;
    stream
        .play()
        .map_err(|error| recorder_error(format!("cannot access audio input stream: {error}")))?;
    drop(stream);
    Ok(())
}

fn input_device(
    host: &cpal::Host,
    selected_name: Option<&str>,
) -> Result<cpal::Device, RecordingError> {
    let Some(selected_name) = selected_name else {
        return host
            .default_input_device()
            .ok_or_else(|| recorder_error("no default audio input device is available"));
    };
    host.input_devices()
        .map_err(|error| recorder_error(format!("cannot enumerate audio input devices: {error}")))?
        .find(|device| device.name().ok().as_deref() == Some(selected_name))
        .ok_or_else(|| {
            recorder_error(format!(
                "selected audio input device is unavailable: {selected_name}"
            ))
        })
}

fn inspect_input_device(device: cpal::Device, name: String) -> InputDeviceInfo {
    let issue = match device.default_input_config() {
        Err(_) => Some(InputDeviceIssue::DefaultConfigUnavailable),
        Ok(default) if default.sample_rate().0 >= MIN_RECORDING_SAMPLE_RATE => None,
        Ok(default) => match device.supported_input_configs() {
            Err(_) => Some(InputDeviceIssue::SupportedConfigUnavailable),
            Ok(ranges) => {
                if select_recording_input_config(&default, ranges).is_some() {
                    None
                } else {
                    Some(InputDeviceIssue::SampleRateTooLow(default.sample_rate().0))
                }
            }
        },
    };
    InputDeviceInfo { name, issue }
}

fn recording_input_config(
    device: &cpal::Device,
) -> Result<cpal::SupportedStreamConfig, RecordingError> {
    let default = device.default_input_config().map_err(|error| {
        recorder_error(format!(
            "cannot query default audio input configuration: {error}"
        ))
    })?;
    if default.sample_rate().0 >= MIN_RECORDING_SAMPLE_RATE {
        return Ok(default);
    }
    let ranges = device.supported_input_configs().map_err(|error| {
        recorder_error(format!(
            "cannot query supported audio input configurations: {error}"
        ))
    })?;
    select_recording_input_config(&default, ranges).ok_or_else(|| {
        recorder_error(format!(
            "selected microphone does not support recording at {MIN_RECORDING_SAMPLE_RATE} Hz or higher"
        ))
    })
}

fn select_recording_input_config(
    default: &cpal::SupportedStreamConfig,
    ranges: impl IntoIterator<Item = cpal::SupportedStreamConfigRange>,
) -> Option<cpal::SupportedStreamConfig> {
    ranges
        .into_iter()
        .filter_map(|range| {
            range.try_with_sample_rate(cpal::SampleRate(
                range.min_sample_rate().0.max(MIN_RECORDING_SAMPLE_RATE),
            ))
        })
        .max_by_key(|config| {
            (
                config.channels() == default.channels()
                    && config.sample_format() == default.sample_format(),
                config.channels() == default.channels(),
                config.sample_format().sample_size(),
                std::cmp::Reverse(config.sample_rate().0),
            )
        })
}

fn input_samples_as_f32<T>(input: &[T]) -> Vec<f32>
where
    T: Sample + Copy,
    f32: FromSample<T>,
{
    input.iter().copied().map(f32::from_sample).collect()
}

fn store_first_error(slot: &Mutex<Option<String>>, message: String) {
    if let Ok(mut error) = slot.lock() {
        if error.is_none() {
            *error = Some(message);
        }
    }
}

fn run_flac_worker(
    receiver: Receiver<Vec<f32>>,
    temporary_path: &Path,
    sample_rate: u32,
    channels: u16,
    samples_per_peak: u32,
    live_waveform: Arc<RwLock<WaveformData>>,
) -> Result<WorkerSummary, RecordingError> {
    let mut command = crate::media_binary::command("ffmpeg");
    command
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-nostdin")
        .arg("-n")
        .arg("-f")
        .arg("f32le")
        .arg("-ar")
        .arg(sample_rate.to_string())
        .arg("-ac")
        .arg(channels.to_string())
        .arg("-i")
        .arg("pipe:0")
        .arg("-vn")
        .arg("-c:a")
        .arg("flac")
        .arg("-f")
        .arg("flac")
        .arg(temporary_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| recorder_error(format!("cannot start FFmpeg recorder: {error}")))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| recorder_error("FFmpeg recorder has no standard input"))?;

    let mut waveform = WaveformAccumulator::new(channels, samples_per_peak);
    let mut published_complete_peaks = 0_usize;
    let mut scalar_sample_count = 0_u64;
    let write_result = (|| -> Result<(), RecordingError> {
        for samples in receiver {
            waveform.push_interleaved(&samples);
            scalar_sample_count = scalar_sample_count.saturating_add(samples.len() as u64);
            let bytes = f32_samples_as_le_bytes(&samples);
            stdin.write_all(&bytes).map_err(|error| {
                recorder_error(format!(
                    "cannot stream microphone samples to FFmpeg: {error}"
                ))
            })?;
            publish_live_waveform(&waveform, &live_waveform, &mut published_complete_peaks);
        }
        Ok(())
    })();
    drop(stdin);

    let output = child
        .wait_with_output()
        .map_err(|error| recorder_error(format!("cannot wait for FFmpeg recorder: {error}")))?;
    write_result?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let detail = if stderr.is_empty() {
            format!("exit status {}", output.status)
        } else {
            stderr
        };
        return Err(recorder_error(format!("FFmpeg recording failed: {detail}")));
    }
    if scalar_sample_count % u64::from(channels) != 0 {
        return Err(recorder_error(
            "audio input ended in the middle of an interleaved sample frame",
        ));
    }

    // Hash the encoded FLAC bytes, not the transient PCM stream, so archive
    // and network receivers can verify the file without decoding it.
    let checksum = sha1_file(temporary_path)?;
    let waveform = waveform.finish();
    if let Ok(mut published) = live_waveform.write() {
        *published = waveform.clone();
    }
    Ok(WorkerSummary {
        sample_rate,
        channels,
        sample_count: scalar_sample_count / u64::from(channels),
        checksum,
        waveform,
    })
}

fn publish_live_waveform(
    waveform: &WaveformAccumulator,
    target: &RwLock<WaveformData>,
    published_complete_peaks: &mut usize,
) {
    let Ok(mut published) = target.write() else {
        return;
    };
    published.samples_per_peak = waveform.samples_per_peak;
    // The last published value may be the in-progress bucket. Remove only
    // that value, append newly completed buckets, then publish a fresh tail.
    published.peaks.truncate(*published_complete_peaks);
    published
        .peaks
        .extend_from_slice(&waveform.peaks[*published_complete_peaks..]);
    *published_complete_peaks = waveform.peaks.len();
    if let Some(partial_peak) = waveform.partial_peak() {
        published.peaks.push(partial_peak);
    }
}

#[derive(Debug, Clone)]
struct WaveformAccumulator {
    channels: u16,
    samples_per_peak: u32,
    channel_index: u16,
    frame_peak: f32,
    frames_in_bucket: u32,
    bucket_peak: f32,
    peaks: Vec<f32>,
}

impl WaveformAccumulator {
    fn new(channels: u16, samples_per_peak: u32) -> Self {
        Self {
            channels: channels.max(1),
            samples_per_peak: samples_per_peak.max(1),
            channel_index: 0,
            frame_peak: 0.0,
            frames_in_bucket: 0,
            bucket_peak: 0.0,
            peaks: Vec::new(),
        }
    }

    fn push_interleaved(&mut self, samples: &[f32]) {
        for &sample in samples {
            let amplitude = if sample.is_finite() {
                sample.abs().min(1.0)
            } else {
                0.0
            };
            self.frame_peak = self.frame_peak.max(amplitude);
            self.channel_index += 1;
            if self.channel_index == self.channels {
                self.channel_index = 0;
                self.bucket_peak = self.bucket_peak.max(self.frame_peak);
                self.frame_peak = 0.0;
                self.frames_in_bucket += 1;
                if self.frames_in_bucket == self.samples_per_peak {
                    self.peaks.push(self.bucket_peak);
                    self.frames_in_bucket = 0;
                    self.bucket_peak = 0.0;
                }
            }
        }
    }

    fn snapshot(&self) -> WaveformData {
        let mut peaks = self.peaks.clone();
        if let Some(partial_peak) = self.partial_peak() {
            peaks.push(partial_peak);
        }
        WaveformData {
            samples_per_peak: self.samples_per_peak,
            peaks,
        }
    }

    fn finish(self) -> WaveformData {
        self.snapshot()
    }

    fn partial_peak(&self) -> Option<f32> {
        (self.frames_in_bucket > 0 || self.channel_index > 0)
            .then(|| self.bucket_peak.max(self.frame_peak))
    }
}

fn sha1_reader(mut reader: impl Read) -> std::io::Result<String> {
    let mut digest = Sha1::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(digest.finalize_hex())
}

fn sha1_file(path: &Path) -> Result<String, RecordingError> {
    let file = fs::File::open(path).map_err(|error| {
        recorder_error(format!(
            "cannot open encoded recording {} for verification: {error}",
            path.display()
        ))
    })?;
    sha1_reader(file).map_err(|error| {
        recorder_error(format!(
            "cannot checksum encoded recording {}: {error}",
            path.display()
        ))
    })
}

fn f32_samples_as_le_bytes(samples: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(samples.len() * std::mem::size_of::<f32>());
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

fn temporary_path_for(output_path: &Path, nonce: u64) -> PathBuf {
    let stem = output_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .unwrap_or("recording");
    let temporary_name = format!("{stem}.recording-{}-{nonce}.part.flac", std::process::id());
    output_path.with_file_name(temporary_name)
}

fn remove_if_present(path: &Path) {
    if let Err(error) = fs::remove_file(path) {
        if error.kind() != std::io::ErrorKind::NotFound {
            log::warn!(
                "Cannot remove partial recording {}: {error}",
                path.display()
            );
        }
    }
}

fn recorder_error(message: impl Into<String>) -> RecordingError {
    RecordingError::Recorder(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_wav_is_imported_as_portable_flac() {
        if !crate::media_binary::can_run("ffmpeg") {
            return;
        }
        let dir = std::env::temp_dir().join(format!(
            "coquerythmo-audio-import-{}-{}",
            std::process::id(),
            TEMP_FILE_NONCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        let source = dir.join("source.wav");
        let output = dir
            .join("missing-output-directory")
            .join("alice_2026-08-12_14-30-00.flac");
        let samples = [0_i16; 800];
        let data_len = (samples.len() * 2) as u32;
        let mut wav = Vec::with_capacity(44 + data_len as usize);
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data_len).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16_u32.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&8_000_u32.to_le_bytes());
        wav.extend_from_slice(&16_000_u32.to_le_bytes());
        wav.extend_from_slice(&2_u16.to_le_bytes());
        wav.extend_from_slice(&16_u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_len.to_le_bytes());
        wav.extend(samples.iter().flat_map(|sample| sample.to_le_bytes()));
        fs::write(&source, wav).unwrap();

        let audio = import_audio(&source, &output).unwrap();
        assert_eq!(audio.file_name, "alice_2026-08-12_14-30-00.flac");
        assert_eq!((audio.sample_rate, audio.channels), (48_000, 1));
        assert!(audio.sample_count > 0 && output.is_file());
        assert_eq!(audio.checksum.len(), 40);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn waveform_uses_loudest_channel_and_exact_frame_buckets() {
        let mut waveform = WaveformAccumulator::new(2, 2);
        waveform.push_interleaved(&[
            0.1, -0.4, // frame 1 => .4
            0.7, 0.2, // frame 2 => .7, closes bucket
            -0.3, 0.1, // frame 3 => .3, partial bucket
        ]);
        assert_eq!(
            waveform.snapshot(),
            WaveformData {
                samples_per_peak: 2,
                peaks: vec![0.7, 0.3],
            }
        );
        assert_eq!(waveform.finish().peaks, vec![0.7, 0.3]);
    }

    #[test]
    fn waveform_preserves_channel_alignment_across_chunks() {
        let mut waveform = WaveformAccumulator::new(2, 1);
        waveform.push_interleaved(&[0.2]);
        assert_eq!(waveform.snapshot().peaks, vec![0.2]);
        waveform.push_interleaved(&[-0.8, 0.1, 0.4]);
        assert_eq!(waveform.finish().peaks, vec![0.8, 0.4]);
    }

    #[test]
    fn waveform_sanitizes_non_finite_and_out_of_range_samples() {
        let mut waveform = WaveformAccumulator::new(1, 3);
        waveform.push_interleaved(&[f32::NAN, -4.0, 0.25]);
        assert_eq!(waveform.finish().peaks, vec![1.0]);
    }

    #[test]
    fn temporary_file_is_a_distinct_flac_in_the_same_directory() {
        let output = Path::new("project/audio/take.flac");
        let temporary = temporary_path_for(output, 42);
        assert_eq!(temporary.parent(), output.parent());
        assert_ne!(temporary, output);
        assert_eq!(
            temporary.extension().and_then(|ext| ext.to_str()),
            Some("flac")
        );
        assert!(temporary
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.contains("recording-") && name.contains("-42.part")));
    }

    #[test]
    fn pcm_byte_encoding_and_checksum_are_deterministic() {
        let bytes = f32_samples_as_le_bytes(&[0.0, 1.0, -0.5]);
        assert_eq!(&bytes[0..4], &0.0_f32.to_le_bytes());
        assert_eq!(&bytes[4..8], &1.0_f32.to_le_bytes());
        assert_eq!(&bytes[8..12], &(-0.5_f32).to_le_bytes());

        let checksum = sha1_reader(std::io::Cursor::new(&bytes)).unwrap();
        assert_eq!(checksum.len(), 40);
        assert!(checksum.bytes().all(|byte| byte.is_ascii_hexdigit()));
    }

    #[test]
    fn f32_microphone_samples_are_forwarded_unchanged() {
        let input = [-1.0_f32, -0.25, 0.0, 0.75, 1.0];
        assert_eq!(input_samples_as_f32(&input), input);
    }

    #[test]
    fn recording_requires_a_real_input_rate_of_at_least_48_khz() {
        let default = cpal::SupportedStreamConfig::new(
            1,
            cpal::SampleRate(8_000),
            cpal::SupportedBufferSize::Unknown,
            cpal::SampleFormat::I16,
        );
        let ranges = [
            cpal::SupportedStreamConfigRange::new(
                1,
                cpal::SampleRate(8_000),
                cpal::SampleRate(8_000),
                cpal::SupportedBufferSize::Unknown,
                cpal::SampleFormat::I16,
            ),
            cpal::SupportedStreamConfigRange::new(
                1,
                cpal::SampleRate(44_100),
                cpal::SampleRate(96_000),
                cpal::SupportedBufferSize::Unknown,
                cpal::SampleFormat::I16,
            ),
        ];

        let selected = select_recording_input_config(&default, ranges).unwrap();

        assert_eq!(selected.sample_rate().0, 48_000);
        assert_eq!(selected.channels(), 1);
        assert_eq!(selected.sample_format(), cpal::SampleFormat::I16);
        assert!(select_recording_input_config(&default, ranges[..1].iter().copied()).is_none());

        let high_rate_only = [cpal::SupportedStreamConfigRange::new(
            1,
            cpal::SampleRate(96_000),
            cpal::SampleRate(96_000),
            cpal::SupportedBufferSize::Unknown,
            cpal::SampleFormat::I16,
        )];
        assert_eq!(
            select_recording_input_config(&default, high_rate_only)
                .unwrap()
                .sample_rate()
                .0,
            96_000
        );
    }
}
