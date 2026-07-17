use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex, RwLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SizedSample};

const VIDEO_PIX_FMT: &str = "bgra";
const VIDEO_TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8UnormSrgb;
const AUDIO_DECODE_CHANNELS: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioTrack {
    Source,
    Instrumental,
}

fn ffmpeg_command() -> Command {
    crate::media_binary::command("ffmpeg")
}

fn ffprobe_command() -> Command {
    crate::media_binary::command("ffprobe")
}

pub struct VideoFrame {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

pub struct VideoPlayer {
    pub texture: Option<wgpu::Texture>,
    pub texture_view: Option<wgpu::TextureView>,
    pub bind_group: Option<wgpu::BindGroup>,

    width: u32,
    height: u32,
    fps: f64,
    playing: bool,
    playback_start_time: Option<Instant>,
    playback_start_frame: i64,
    playback_start_audio_frame: u64,
    last_toggle: Option<Instant>,
    receiver: Option<Receiver<VideoFrame>>,
    frame_recycler: Option<SyncSender<Vec<u8>>>,
    decoder_handle: Option<JoinHandle<()>>,
    kill_signal: Arc<AtomicBool>,
    path: Option<PathBuf>,
    source_audio_path: Option<PathBuf>,
    instrumental_audio_path: Option<PathBuf>,
    active_audio_track: AudioTrack,
    source_audio_offset_frames: i64,
    instrumental_audio_offset_frames: i64,
    finished: bool,
    current_frame: i64,
    total_frames: i64,
    volume: f32,

    audio_stream: Option<cpal::Stream>,
    audio_clock: Option<Arc<AudioOutputState>>,
    audio_thread: Option<JoinHandle<()>>,
    pub waveform: Arc<RwLock<Vec<f32>>>,
    pub instrumental_waveform: Arc<RwLock<Vec<f32>>>,
    waveform_revision: Arc<AtomicU64>,
    waveform_jobs: Arc<AtomicU32>,
}

impl Default for VideoPlayer {
    fn default() -> Self {
        Self::new()
    }
}

struct AudioClockSnapshot {
    wall_instant: Instant,
    audible_frame: i64,
    written_frame: u64,
}

struct AudioOutputState {
    sample_rate: u32,
    volume_bits: AtomicU32,
    frames_written: AtomicU64,
    underruns: AtomicU64,
    snapshot: Mutex<AudioClockSnapshot>,
}

impl AudioOutputState {
    fn new(sample_rate: u32, volume: f32) -> Self {
        Self {
            sample_rate,
            volume_bits: AtomicU32::new(volume.to_bits()),
            frames_written: AtomicU64::new(0),
            underruns: AtomicU64::new(0),
            snapshot: Mutex::new(AudioClockSnapshot {
                wall_instant: Instant::now(),
                audible_frame: 0,
                written_frame: 0,
            }),
        }
    }

    fn set_volume(&self, volume: f32) {
        self.volume_bits.store(volume.to_bits(), Ordering::Relaxed);
    }

    fn volume(&self) -> f32 {
        f32::from_bits(self.volume_bits.load(Ordering::Relaxed))
    }

    fn audible_frame(&self) -> u64 {
        let Ok(snapshot) = self.snapshot.lock() else {
            return self.frames_written.load(Ordering::Relaxed);
        };
        self.audible_frame_from_snapshot(&snapshot, Instant::now())
    }

    fn freeze(&self) {
        if let Ok(mut snapshot) = self.snapshot.lock() {
            let now = Instant::now();
            let audible_frame = self.audible_frame_from_snapshot(&snapshot, now);
            snapshot.wall_instant = now;
            snapshot.audible_frame = audible_frame as i64;
            snapshot.written_frame = self.frames_written.load(Ordering::Relaxed);
        }
    }

    fn update_callback_snapshot(&self, frames_before: u64, frames_after: u64, latency_frames: u64) {
        if let Ok(mut snapshot) = self.snapshot.lock() {
            snapshot.wall_instant = Instant::now();
            snapshot.audible_frame = frames_before as i64 - latency_frames as i64;
            snapshot.written_frame = frames_after;
        }
    }

    fn audible_frame_from_snapshot(&self, snapshot: &AudioClockSnapshot, now: Instant) -> u64 {
        let elapsed_frames = now
            .saturating_duration_since(snapshot.wall_instant)
            .as_secs_f64()
            * self.sample_rate as f64;
        let audible = snapshot.audible_frame as f64 + elapsed_frames.max(0.0);
        if audible <= 0.0 {
            0
        } else {
            (audible as u64).min(snapshot.written_frame)
        }
    }
}

impl VideoPlayer {
    pub fn new() -> Self {
        Self {
            texture: None,
            texture_view: None,
            bind_group: None,
            width: 0,
            height: 0,
            fps: 30.0,
            playing: false,
            playback_start_time: None,
            playback_start_frame: 0,
            playback_start_audio_frame: 0,
            last_toggle: None,
            receiver: None,
            frame_recycler: None,
            decoder_handle: None,
            kill_signal: Arc::new(AtomicBool::new(false)),
            path: None,
            source_audio_path: None,
            instrumental_audio_path: None,
            active_audio_track: AudioTrack::Source,
            source_audio_offset_frames: 0,
            instrumental_audio_offset_frames: 0,
            finished: false,
            current_frame: 0,
            total_frames: 0,
            volume: 0.75,
            audio_stream: None,
            audio_clock: None,
            audio_thread: None,
            waveform: Arc::new(RwLock::new(Vec::new())),
            instrumental_waveform: Arc::new(RwLock::new(Vec::new())),
            waveform_revision: Arc::new(AtomicU64::new(0)),
            waveform_jobs: Arc::new(AtomicU32::new(0)),
        }
    }

    pub fn load_with_audio(
        &mut self,
        video_path: &Path,
        audio_path: &Path,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bind_group_layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
    ) -> Result<(), String> {
        self.stop();

        let (width, height, fps, total_frames) = probe_video(video_path)?;
        self.width = width;
        self.height = height;
        self.fps = fps;
        self.total_frames = total_frames;
        self.current_frame = 0;
        self.path = Some(video_path.to_path_buf());
        self.source_audio_path = Some(audio_path.to_path_buf());
        self.active_audio_track = AudioTrack::Source;
        self.finished = false;

        log::info!(
            "Video: {}x{} @ {:.2} fps, {} frames from {} (audio from {})",
            width,
            height,
            fps,
            total_frames,
            video_path.display(),
            audio_path.display()
        );

        let first_frame = decode_frame_at(video_path, width, height, 0.0)?;
        self.upload_frame(&first_frame, device, queue, bind_group_layout, sampler);

        self.start_decoders_at(0.0);

        self.decode_source_waveform();

        Ok(())
    }

    pub fn set_volume(&mut self, vol: f32) {
        self.volume = vol;
        if let Some(clock) = &self.audio_clock {
            clock.set_volume(if vol < 0.01 { 0.0 } else { vol });
        }
    }

    pub fn active_audio_track(&self) -> AudioTrack {
        self.active_audio_track
    }

    pub fn active_audio_offset_frames(&self) -> i64 {
        match self.active_audio_track {
            AudioTrack::Source => self.source_audio_offset_frames,
            AudioTrack::Instrumental => self.instrumental_audio_offset_frames,
        }
    }

    pub fn set_instrumental_audio_path(&mut self, path: Option<PathBuf>) {
        self.instrumental_audio_path = path;
        if self.active_audio_track == AudioTrack::Instrumental
            && self.instrumental_audio_path.is_none()
        {
            self.active_audio_track = AudioTrack::Source;
        }
        self.decode_instrumental_waveform();
        if self.active_audio_track == AudioTrack::Instrumental {
            self.reload_audio_at_current_frame();
        }
    }

    pub fn set_audio_offsets(&mut self, source_frames: i64, instrumental_frames: i64) {
        self.source_audio_offset_frames = source_frames;
        self.instrumental_audio_offset_frames = instrumental_frames;
        self.reload_audio_at_current_frame();
    }

    pub fn adjust_active_audio_offset(&mut self, delta_frames: i64) {
        match self.active_audio_track {
            AudioTrack::Source => self.source_audio_offset_frames += delta_frames,
            AudioTrack::Instrumental => self.instrumental_audio_offset_frames += delta_frames,
        }
        self.reload_audio_at_current_frame();
    }

    pub fn toggle_audio_track(&mut self) -> bool {
        if self.instrumental_audio_path.is_none() {
            return false;
        }
        self.active_audio_track = match self.active_audio_track {
            AudioTrack::Source => AudioTrack::Instrumental,
            AudioTrack::Instrumental => AudioTrack::Source,
        };
        self.ensure_active_waveform();
        self.reload_audio_at_current_frame();
        true
    }

    pub fn waveform_for_render(&self) -> Arc<RwLock<Vec<f32>>> {
        match self.active_audio_track {
            AudioTrack::Source => self.waveform.clone(),
            AudioTrack::Instrumental => self.instrumental_waveform.clone(),
        }
    }

    pub fn waveform_revision(&self) -> u64 {
        self.waveform_revision.load(Ordering::Relaxed)
    }

    pub fn is_waveform_decoding(&self) -> bool {
        self.waveform_jobs.load(Ordering::Relaxed) > 0
    }

    pub fn toggle(&mut self) -> bool {
        if self.finished {
            return false;
        }

        // Debounce: ignore toggles within 200ms of each other
        let now = Instant::now();
        if let Some(last) = self.last_toggle {
            if now.duration_since(last).as_millis() < 50 {
                return false;
            }
        }
        self.last_toggle = Some(now);

        self.playing = !self.playing;
        if self.playing {
            if self.receiver.is_none() {
                let ts = self.current_frame as f64 / self.fps;
                self.start_decoders_at(ts);
            }

            self.playback_start_time = Some(now);
            self.playback_start_frame = self.current_frame;
            self.playback_start_audio_frame = self
                .audio_clock
                .as_ref()
                .map(|clock| clock.audible_frame())
                .unwrap_or(0);

            if let Some(stream) = &self.audio_stream {
                if let Err(e) = stream.play() {
                    log::error!("Audio stream play failed: {e}");
                }
            }
        } else {
            if let Some(clock) = &self.audio_clock {
                clock.freeze();
            }
            if let Some(stream) = &self.audio_stream {
                if let Err(e) = stream.pause() {
                    log::warn!("Audio stream pause failed: {e}");
                }
            }
        }

        true
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// Stop playback and discard its buffered frames before an interactive seek.
    pub fn pause_for_seek(&mut self) -> bool {
        let was_playing = self.playing;
        if was_playing {
            self.playing = false;
            self.stop_decoders();
        }
        was_playing
    }

    /// Move current_frame by delta WITHOUT decoding (instant, for scroll).
    ///
    /// Seeking always invalidates the decoder streams, including while paused:
    /// otherwise pressing play before the debounced frame decode can reuse the
    /// old audio stream and start it at the previous video position.
    pub fn seek_frame_instant(&mut self, delta: i32) {
        let was_playing = self.playing;
        // The decoder FIFOs still contain frames/audio from the old position.
        // Drop them now and let the debounced seek callback (or the next play)
        // start fresh streams from the new position.
        self.stop_decoders();

        let target = (self.current_frame + delta as i64).max(0);
        let target = if self.total_frames > 0 {
            target.min(self.total_frames - 1)
        } else {
            target
        };
        self.current_frame = target;
        self.finished = false;

        if was_playing {
            self.playback_start_time = Some(Instant::now());
            self.playback_start_frame = self.current_frame;
            self.playback_start_audio_frame = 0;
        }
    }

    /// Restart the audio/video streams after a debounced seek made during
    /// playback. The current frame remains the clock origin while the new
    /// decoder catches up.
    pub fn restart_playback_decoders(&mut self) {
        if !self.playing || self.receiver.is_some() {
            return;
        }

        let timestamp = self.current_frame as f64 / self.fps.max(1.0);
        self.start_decoders_at(timestamp);
        self.playback_start_time = Some(Instant::now());
        self.playback_start_frame = self.current_frame;
        self.playback_start_audio_frame = self
            .audio_clock
            .as_ref()
            .map(|clock| clock.audible_frame())
            .unwrap_or(0);
    }

    /// Decode and display the frame at current_frame. Call after scroll stabilizes.
    pub fn decode_current_frame(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bind_group_layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
    ) {
        let path = match &self.path {
            Some(p) => p.clone(),
            None => return,
        };

        let timestamp = self.current_frame as f64 / self.fps;
        if let Ok(frame) = decode_frame_at(&path, self.width, self.height, timestamp) {
            self.upload_frame(&frame, device, queue, bind_group_layout, sampler);
        }

        self.stop_decoders();
        self.finished = false;
    }

    /// Full seek: move + decode (for step forward/backward buttons).
    pub fn seek_relative(
        &mut self,
        delta: i32,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bind_group_layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
    ) {
        self.seek_frame_instant(delta);
        self.decode_current_frame(device, queue, bind_group_layout, sampler);
    }

    pub fn step_forward(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bind_group_layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
    ) {
        self.playing = false;
        self.seek_relative(1, device, queue, bind_group_layout, sampler);
    }

    pub fn step_backward(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bind_group_layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
    ) {
        self.playing = false;
        self.seek_relative(-1, device, queue, bind_group_layout, sampler);
    }

    pub fn tick(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bind_group_layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
    ) {
        if !self.playing {
            return;
        }

        let now = Instant::now();
        let Some(target_playback_frame) = self.playback_frame_at(now) else {
            return;
        };
        let target_render_frame = self.clamp_render_frame(target_playback_frame);
        let target_frame = target_render_frame.floor() as i64;

        let wall_clock_frame = self.playback_start_time.map(|start| {
            self.playback_start_frame as f64 + start.elapsed().as_secs_f64() * self.fps
        });
        if self.total_frames > 0
            && (target_playback_frame >= self.total_frames as f64
                || wall_clock_frame.is_some_and(|frame| frame >= self.total_frames as f64))
        {
            self.playing = false;
            self.finished = true;
            self.stop_decoders();
            log::info!("Video playback finished");
            return;
        }

        // Already at or ahead of target — nothing to do
        if self.current_frame >= target_frame {
            return;
        }

        // Consume frames from channel to catch up
        let mut last_frame = None;
        let frames_behind = (target_frame - self.current_frame) as usize;
        if let Some(rx) = &self.receiver {
            for _ in 0..frames_behind {
                match rx.try_recv() {
                    Ok(frame) => {
                        if let Some(previous) = last_frame.replace(frame) {
                            recycle_frame(previous, self.frame_recycler.as_ref());
                        }
                        self.current_frame += 1;
                    }
                    Err(mpsc::TryRecvError::Empty) => break, // decoder hasn't caught up
                    Err(mpsc::TryRecvError::Disconnected) => {
                        self.playing = false;
                        self.finished = true;
                        self.receiver = None;
                        if let Some(stream) = &self.audio_stream {
                            let _ = stream.pause();
                        }
                        log::info!("Video playback finished");
                        return;
                    }
                }
            }
        }

        // Upload only the last consumed frame (skip intermediate ones for performance)
        if let Some(frame) = last_frame {
            self.upload_frame(&frame, device, queue, bind_group_layout, sampler);
            recycle_frame(frame, self.frame_recycler.as_ref());
        }
    }

    pub fn current_frame(&self) -> i64 {
        self.current_frame
    }

    fn playback_elapsed_seconds_at(&self, now: Instant) -> Option<f64> {
        let start_time = self.playback_start_time?;
        if let Some(clock) = &self.audio_clock {
            let audio_frames = clock
                .audible_frame()
                .saturating_sub(self.playback_start_audio_frame);
            Some(audio_frames as f64 / clock.sample_rate as f64)
        } else {
            Some(now.duration_since(start_time).as_secs_f64())
        }
    }

    fn clamp_render_frame(&self, frame: f64) -> f64 {
        if self.total_frames > 0 {
            frame.clamp(0.0, (self.total_frames - 1) as f64)
        } else {
            frame.max(0.0)
        }
    }

    fn playback_frame_at(&self, now: Instant) -> Option<f64> {
        let elapsed = self.playback_elapsed_seconds_at(now)?;
        Some(self.playback_start_frame as f64 + elapsed * self.fps)
    }

    fn playback_render_frame_at(&self, now: Instant) -> Option<f64> {
        Some(self.clamp_render_frame(self.playback_frame_at(now)?))
    }

    /// Visual frame for UI rendering. Decoded-frame and timeline state remain integer-based.
    pub fn current_frame_for_render(&self) -> f64 {
        if !self.playing {
            return self.current_frame as f64;
        }
        self.playback_render_frame_at(Instant::now())
            .unwrap_or(self.current_frame as f64)
    }

    pub fn fps(&self) -> f64 {
        self.fps
    }

    pub fn path(&self) -> Option<std::path::PathBuf> {
        self.path.clone()
    }

    pub fn total_frames(&self) -> i64 {
        self.total_frames
    }

    pub fn video_size(&self) -> Option<(u32, u32)> {
        if self.width > 0 && self.height > 0 {
            Some((self.width, self.height))
        } else {
            None
        }
    }

    fn start_decoders_at(&mut self, timestamp: f64) {
        let path = match &self.path {
            Some(p) => p.clone(),
            None => return,
        };
        let audio_path = self.active_audio_path().unwrap_or_else(|| path.clone());

        // Fresh kill signal for new decoders
        self.kill_signal = Arc::new(AtomicBool::new(false));

        // Video decoder
        let frame_size = (self.width * self.height * 4) as usize;
        let (tx, rx) = mpsc::sync_channel::<VideoFrame>(3);
        let (free_tx, free_rx) = mpsc::sync_channel::<Vec<u8>>(3);
        for _ in 0..3 {
            if free_tx.send(vec![0u8; frame_size]).is_err() {
                break;
            }
        }
        let path_clone = path.clone();
        let w = self.width;
        let h = self.height;
        let kill = self.kill_signal.clone();
        let vid_handle = thread::spawn(move || {
            decode_video_stream_from(path_clone, w, h, timestamp, tx, free_rx, kill);
        });
        self.receiver = Some(rx);
        self.frame_recycler = Some(free_tx);
        self.decoder_handle = Some(vid_handle);

        // Audio decoder
        let audio_timestamp = self.audio_timestamp_for_video_timestamp(timestamp);
        if self.setup_audio_from(&audio_path, audio_timestamp).is_ok() {
            self.set_volume(self.volume);
            if !self.playing {
                if let Some(stream) = &self.audio_stream {
                    let _ = stream.pause();
                }
            }
        }
    }

    fn active_audio_path(&self) -> Option<PathBuf> {
        match self.active_audio_track {
            AudioTrack::Source => self.source_audio_path.clone(),
            AudioTrack::Instrumental => self
                .instrumental_audio_path
                .clone()
                .or_else(|| self.source_audio_path.clone()),
        }
    }

    fn audio_timestamp_for_video_timestamp(&self, timestamp: f64) -> f64 {
        let offset_secs = self.active_audio_offset_frames() as f64 / self.fps.max(1.0);
        (timestamp - offset_secs).max(0.0)
    }

    fn reload_audio_at_current_frame(&mut self) {
        let Some(audio_path) = self.active_audio_path() else {
            return;
        };
        let was_playing = self.playing;
        if let Some(clock) = &self.audio_clock {
            clock.freeze();
        }
        if let Some(stream) = &self.audio_stream {
            let _ = stream.pause();
        }
        self.audio_stream = None;
        self.audio_clock = None;
        self.audio_thread.take();

        let timestamp =
            self.audio_timestamp_for_video_timestamp(self.current_frame as f64 / self.fps);
        if self.setup_audio_from(&audio_path, timestamp).is_ok() {
            self.set_volume(self.volume);
            if was_playing {
                self.playback_start_time = Some(Instant::now());
                self.playback_start_frame = self.current_frame;
                self.playback_start_audio_frame = self
                    .audio_clock
                    .as_ref()
                    .map(|clock| clock.audible_frame())
                    .unwrap_or(0);
                if let Some(stream) = &self.audio_stream {
                    let _ = stream.play();
                }
            } else if let Some(stream) = &self.audio_stream {
                let _ = stream.pause();
            }
        }
    }

    fn ensure_active_waveform(&mut self) {
        match self.active_audio_track {
            AudioTrack::Source => {
                let is_empty = self.waveform.read().map(|w| w.is_empty()).unwrap_or(true);
                if is_empty {
                    self.decode_source_waveform();
                }
            }
            AudioTrack::Instrumental => {
                let is_empty = self
                    .instrumental_waveform
                    .read()
                    .map(|w| w.is_empty())
                    .unwrap_or(true);
                if is_empty {
                    self.decode_instrumental_waveform();
                }
            }
        }
    }

    fn decode_source_waveform(&mut self) {
        let had_data = if let Ok(mut w) = self.waveform.write() {
            let had_data = !w.is_empty();
            w.clear();
            had_data
        } else {
            false
        };
        if had_data {
            self.waveform_revision.fetch_add(1, Ordering::Relaxed);
        }
        let Some(wave_path) = self.source_audio_path.clone() else {
            return;
        };
        self.spawn_waveform_decode(wave_path, self.waveform.clone());
    }

    fn decode_instrumental_waveform(&mut self) {
        let had_data = if let Ok(mut w) = self.instrumental_waveform.write() {
            let had_data = !w.is_empty();
            w.clear();
            had_data
        } else {
            false
        };
        if had_data {
            self.waveform_revision.fetch_add(1, Ordering::Relaxed);
        }
        let Some(wave_path) = self.instrumental_audio_path.clone() else {
            return;
        };
        self.spawn_waveform_decode(wave_path, self.instrumental_waveform.clone());
    }

    fn spawn_waveform_decode(&self, wave_path: PathBuf, waveform: Arc<RwLock<Vec<f32>>>) {
        let wave_fps = self.fps;
        let wave_total = self.total_frames;
        let waveform_revision = self.waveform_revision.clone();
        let waveform_jobs = self.waveform_jobs.clone();
        waveform_jobs.fetch_add(1, Ordering::Relaxed);
        thread::spawn(move || {
            match decode_waveform_peaks(&wave_path, wave_fps, wave_total as usize) {
                Ok(data) => {
                    let decoded_len = data.len();
                    if let Ok(mut w) = waveform.write() {
                        *w = data;
                        waveform_revision.fetch_add(1, Ordering::Relaxed);
                    }
                    log::info!("Waveform decoded: {} peaks", decoded_len);
                }
                Err(e) => log::warn!("Waveform decode failed for {}: {e}", wave_path.display()),
            }
            waveform_jobs.fetch_sub(1, Ordering::Relaxed);
        });
    }

    fn setup_audio_from(&mut self, path: &Path, timestamp: f64) -> Result<(), String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| "No default audio output device".to_string())?;
        let supported = device
            .default_output_config()
            .map_err(|e| format!("Audio output config failed: {e}"))?;
        let config = supported.config();
        let sample_rate = config.sample_rate.0;
        let channels = config.channels as usize;

        let (audio_tx, audio_rx) = mpsc::sync_channel::<Vec<f32>>(24);
        let path_clone = path.to_path_buf();
        let audio_handle = thread::spawn(move || {
            decode_audio_stream_from(path_clone, timestamp, sample_rate, audio_tx);
        });
        let initial_chunk = match audio_rx.recv_timeout(Duration::from_millis(200)) {
            Ok(chunk) => Some(chunk),
            Err(mpsc::RecvTimeoutError::Timeout) => None,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("No decodable audio stream".to_string());
            }
        };

        let state = Arc::new(AudioOutputState::new(sample_rate, self.volume));
        let stream = match supported.sample_format() {
            cpal::SampleFormat::F32 => build_audio_stream::<f32>(
                &device,
                &config,
                audio_rx,
                initial_chunk,
                state.clone(),
                channels,
            ),
            cpal::SampleFormat::I16 => build_audio_stream::<i16>(
                &device,
                &config,
                audio_rx,
                initial_chunk,
                state.clone(),
                channels,
            ),
            cpal::SampleFormat::U16 => build_audio_stream::<u16>(
                &device,
                &config,
                audio_rx,
                initial_chunk,
                state.clone(),
                channels,
            ),
            sample_format => Err(format!(
                "Unsupported audio sample format: {sample_format:?}"
            )),
        }?;

        self.audio_stream = Some(stream);
        self.audio_clock = Some(state);
        self.audio_thread = Some(audio_handle);
        Ok(())
    }

    fn upload_frame(
        &mut self,
        frame: &VideoFrame,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bind_group_layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
    ) {
        let need_new = match &self.texture {
            Some(tex) => tex.width() != frame.width || tex.height() != frame.height,
            None => true,
        };

        if need_new {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Video Frame"),
                size: wgpu::Extent3d {
                    width: frame.width,
                    height: frame.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: VIDEO_TEXTURE_FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Video BG"),
                layout: bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(sampler),
                    },
                ],
            });
            self.texture = Some(texture);
            self.texture_view = Some(view);
            self.bind_group = Some(bind_group);
        }

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: self.texture.as_ref().unwrap(),
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &frame.data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * frame.width),
                rows_per_image: Some(frame.height),
            },
            wgpu::Extent3d {
                width: frame.width,
                height: frame.height,
                depth_or_array_layers: 1,
            },
        );
    }

    fn stop_decoders(&mut self) {
        // Signal threads to kill their ffmpeg processes and exit
        self.kill_signal.store(true, Ordering::Relaxed);

        if let Some(stream) = &self.audio_stream {
            let _ = stream.pause();
        }
        self.audio_stream = None;
        self.audio_clock = None;
        self.receiver = None;
        self.frame_recycler = None;
        self.decoder_handle.take();
        self.audio_thread.take();
    }

    fn stop(&mut self) {
        self.playing = false;
        self.stop_decoders();
    }
}

impl Drop for VideoPlayer {
    fn drop(&mut self) {
        self.stop();
    }
}

// --- Direct CPAL audio output ---

const AUDIO_CHANNELS: u16 = AUDIO_DECODE_CHANNELS as u16;

struct AudioSampleReader {
    rx: Receiver<Vec<f32>>,
    buffer: Vec<f32>,
    pos: usize,
}

impl AudioSampleReader {
    fn new(rx: Receiver<Vec<f32>>, initial_chunk: Option<Vec<f32>>) -> Self {
        Self {
            rx,
            buffer: initial_chunk.unwrap_or_default(),
            pos: 0,
        }
    }

    fn next_stereo(&mut self) -> Option<[f32; 2]> {
        let left = self.next_sample()?;
        let right = self.next_sample().unwrap_or(left);
        Some([left, right])
    }

    fn next_sample(&mut self) -> Option<f32> {
        loop {
            if self.pos < self.buffer.len() {
                let sample = self.buffer[self.pos];
                self.pos += 1;
                return Some(sample);
            }
            match self.rx.recv() {
                Ok(chunk) => {
                    self.buffer = chunk;
                    self.pos = 0;
                }
                Err(_) => return None,
            }
        }
    }
}

fn build_audio_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    rx: Receiver<Vec<f32>>,
    initial_chunk: Option<Vec<f32>>,
    state: Arc<AudioOutputState>,
    output_channels: usize,
) -> Result<cpal::Stream, String>
where
    T: Sample + SizedSample + FromSample<f32>,
{
    let sample_rate = config.sample_rate.0;
    let mut reader = AudioSampleReader::new(rx, initial_chunk);
    let err_fn = |err| log::error!("Audio stream error: {err}");

    device
        .build_output_stream(
            config,
            move |output: &mut [T], info: &cpal::OutputCallbackInfo| {
                write_audio_output(
                    output,
                    output_channels,
                    sample_rate,
                    &state,
                    &mut reader,
                    info,
                )
            },
            err_fn,
            None,
        )
        .map_err(|e| format!("Audio stream build failed: {e}"))
}

fn write_audio_output<T>(
    output: &mut [T],
    output_channels: usize,
    sample_rate: u32,
    state: &AudioOutputState,
    reader: &mut AudioSampleReader,
    info: &cpal::OutputCallbackInfo,
) where
    T: Sample + FromSample<f32>,
{
    let frame_count = output.len() / output_channels.max(1);
    let frames_before = state
        .frames_written
        .fetch_add(frame_count as u64, Ordering::Relaxed);
    let frames_after = frames_before + frame_count as u64;
    let latency_frames = info
        .timestamp()
        .playback
        .duration_since(&info.timestamp().callback)
        .map(|duration| (duration.as_secs_f64() * sample_rate as f64).round() as u64)
        .unwrap_or(0);
    state.update_callback_snapshot(frames_before, frames_after, latency_frames);

    let volume = state.volume();
    for frame in output.chunks_mut(output_channels.max(1)) {
        let stereo = match reader.next_stereo() {
            Some(stereo) => stereo,
            None => {
                state.underruns.fetch_add(1, Ordering::Relaxed);
                [0.0, 0.0]
            }
        };
        write_output_frame(frame, stereo, volume);
    }
}

fn write_output_frame<T>(frame: &mut [T], stereo: [f32; 2], volume: f32)
where
    T: Sample + FromSample<f32>,
{
    if frame.len() == 1 {
        frame[0] = T::from_sample(((stereo[0] + stereo[1]) * 0.5 * volume).clamp(-1.0, 1.0));
        return;
    }

    for (idx, sample) in frame.iter_mut().enumerate() {
        let value = if idx % 2 == 0 { stereo[0] } else { stereo[1] };
        *sample = T::from_sample((value * volume).clamp(-1.0, 1.0));
    }
}

fn recycle_frame(frame: VideoFrame, recycler: Option<&SyncSender<Vec<u8>>>) {
    if let Some(recycler) = recycler {
        let _ = recycler.try_send(frame.data);
    }
}

// --- ffmpeg subprocess functions ---

fn probe_video(path: &Path) -> Result<(u32, u32, f64, i64), String> {
    let output = ffprobe_command()
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height,r_frame_rate,nb_frames",
            "-of",
            "csv=p=0:s=,",
        ])
        .arg(path)
        .output()
        .map_err(|e| format!("ffprobe failed: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(format!(
            "ffprobe error (code {:?}):\nstderr: {}\nstdout: {}",
            output.status.code(),
            stderr.trim(),
            stdout.trim()
        ));
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let text = text.trim();
    if text.is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "ffprobe returned empty output for '{}'\nstderr: {}",
            path.display(),
            stderr.trim()
        ));
    }
    let parts: Vec<&str> = text.split(',').collect();
    if parts.len() < 3 {
        return Err(format!(
            "Unexpected ffprobe output: '{text}' for '{}'",
            path.display()
        ));
    }

    let width: u32 = parts[0].parse().map_err(|e| format!("Bad width: {e}"))?;
    let height: u32 = parts[1].parse().map_err(|e| format!("Bad height: {e}"))?;
    let fps = parse_frame_rate(parts[2])?;
    let total_frames: i64 = if parts.len() > 3 {
        parts[3].parse().unwrap_or(0)
    } else {
        0
    };

    Ok((width, height, fps, total_frames))
}

fn parse_frame_rate(s: &str) -> Result<f64, String> {
    let parts: Vec<&str> = s.split('/').collect();
    if parts.len() == 2 {
        let num: f64 = parts[0].parse().map_err(|e| format!("Bad fps num: {e}"))?;
        let den: f64 = parts[1].parse().map_err(|e| format!("Bad fps den: {e}"))?;
        if den > 0.0 {
            Ok(num / den)
        } else {
            Ok(30.0)
        }
    } else {
        s.parse::<f64>().map_err(|e| format!("Bad fps: {e}"))
    }
}

fn decode_frame_at(
    path: &Path,
    width: u32,
    height: u32,
    timestamp: f64,
) -> Result<VideoFrame, String> {
    let ts = format!("{:.6}", timestamp);
    let output = ffmpeg_command()
        .args(["-ss", &ts])
        .arg("-i")
        .arg(path)
        .args([
            "-frames:v",
            "1",
            "-pix_fmt",
            VIDEO_PIX_FMT,
            "-f",
            "rawvideo",
            "-v",
            "error",
            "pipe:1",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("ffmpeg seek failed: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "ffmpeg error: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let expected = (width * height * 4) as usize;
    if output.stdout.len() != expected {
        return Err(format!(
            "Bad frame size: {} vs expected {}",
            output.stdout.len(),
            expected
        ));
    }

    Ok(VideoFrame {
        data: output.stdout,
        width,
        height,
    })
}

fn decode_video_stream_from(
    path: PathBuf,
    width: u32,
    height: u32,
    timestamp: f64,
    tx: SyncSender<VideoFrame>,
    free_rx: Receiver<Vec<u8>>,
    kill: Arc<AtomicBool>,
) {
    let ts = format!("{:.6}", timestamp);
    let mut child = match ffmpeg_command()
        .args(["-ss", &ts])
        .arg("-i")
        .arg(&path)
        .args([
            "-an",
            "-sn",
            "-dn",
            "-pix_fmt",
            VIDEO_PIX_FMT,
            "-f",
            "rawvideo",
            "-v",
            "error",
            "pipe:1",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            log::error!("Failed to spawn ffmpeg video decoder: {e}");
            return;
        }
    };

    let stdout = child.stdout.take().unwrap();
    let mut reader =
        std::io::BufReader::with_capacity(width as usize * height as usize * 4, stdout);
    let frame_size = (width * height * 4) as usize;
    let mut buf = vec![0u8; frame_size];

    // Skip first frame (already shown by decode_frame_at)
    if reader.read_exact(&mut buf).is_err() {
        let _ = child.kill();
        let _ = child.wait();
        return;
    }

    loop {
        // Check kill signal
        if kill.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return;
        }
        let mut frame_data = match free_rx.recv() {
            Ok(buf) => buf,
            Err(_) => break,
        };
        match reader.read_exact(&mut frame_data) {
            Ok(()) => {
                let frame = VideoFrame {
                    data: frame_data,
                    width,
                    height,
                };
                if tx.send(frame).is_err() {
                    break;
                }
            }
            Err(_) => break,
        }
    }

    let _ = child.kill();
    let _ = child.wait();
}

fn decode_audio_stream_from(
    path: PathBuf,
    timestamp: f64,
    sample_rate: u32,
    tx: SyncSender<Vec<f32>>,
) {
    let ts = format!("{:.6}", timestamp);
    let mut child = match ffmpeg_command()
        .args(["-threads", "1"])
        .args(["-ss", &ts])
        .arg("-i")
        .arg(&path)
        .args([
            "-vn",
            "-af",
            "aresample=async=1:first_pts=0",
            "-f",
            "f32le",
            "-acodec",
            "pcm_f32le",
            "-ar",
            &sample_rate.to_string(),
            "-ac",
            &AUDIO_CHANNELS.to_string(),
            "-v",
            "error",
            "pipe:1",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            log::error!("Failed to spawn ffmpeg audio decoder: {e}");
            return;
        }
    };

    let stdout = child.stdout.take().unwrap();
    let mut reader = std::io::BufReader::new(stdout);
    let chunk_samples = 4096 * AUDIO_CHANNELS as usize;
    let chunk_bytes = chunk_samples * 4;
    let mut buf = vec![0u8; chunk_bytes];

    while let Ok(()) = reader.read_exact(&mut buf) {
        let samples: Vec<f32> = buf
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();
        if tx.send(samples).is_err() {
            break;
        }
    }

    let _ = child.kill();
    let _ = child.wait();
}

/// Decode entire audio track and compute peak amplitude.
/// Returns `SAMPLES_PER_FRAME` peaks per video frame for sub-frame precision.
const WAVEFORM_SUBDIVISIONS: usize = 4;

fn decode_waveform_peaks(path: &Path, fps: f64, total_frames: usize) -> Result<Vec<f32>, String> {
    let sr = 22050u32;
    let mut child = ffmpeg_command()
        .args(["-threads", "1"])
        .arg("-i")
        .arg(path)
        .args([
            "-vn",
            "-af",
            "aresample=async=1:first_pts=0",
            "-f",
            "f32le",
            "-acodec",
            "pcm_f32le",
            "-ar",
            &sr.to_string(),
            "-ac",
            "1",
            "-v",
            "error",
            "pipe:1",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("ffmpeg waveform: {e}"))?;

    let stdout = child.stdout.take().unwrap();
    let mut reader = std::io::BufReader::with_capacity(65536, stdout);

    // Sub-frame resolution: WAVEFORM_SUBDIVISIONS peaks per video frame.
    // Use floating-point accumulator to avoid integer truncation drift.
    let sub_fps = fps * WAVEFORM_SUBDIVISIONS as f64;
    let exact_samples_per_sub = sr as f64 / sub_fps;
    let total_subs = total_frames * WAVEFORM_SUBDIVISIONS;
    let mut peaks = Vec::with_capacity(total_subs);
    let max_chunk = (exact_samples_per_sub.ceil() as usize + 1) * 4;
    let mut buf = vec![0u8; max_chunk];
    let mut sample_accum = 0.0_f64;

    for _ in 0..total_subs {
        sample_accum += exact_samples_per_sub;
        let n = sample_accum.round() as usize;
        sample_accum -= n as f64;
        let bytes = n * 4;
        match reader.read_exact(&mut buf[..bytes]) {
            Ok(()) => {
                let peak = buf[..bytes]
                    .chunks_exact(4)
                    .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]).abs())
                    .fold(0.0_f32, f32::max);
                peaks.push(peak);
            }
            Err(_) => {
                peaks.resize(total_subs, 0.0);
                break;
            }
        }
    }

    let _ = child.kill();
    let _ = child.wait();
    Ok(peaks)
}

#[cfg(test)]
mod tests {
    use std::sync::{mpsc, Arc};

    use super::{AudioOutputState, VideoPlayer};

    #[test]
    fn interactive_seek_pauses_active_playback() {
        let mut player = VideoPlayer::new();
        player.playing = true;

        assert!(player.pause_for_seek());
        assert!(!player.is_playing());
        assert!(!player.pause_for_seek());
    }

    #[test]
    fn seek_invalidates_audio_stream_while_paused() {
        let mut player = VideoPlayer::new();
        let (_, receiver) = mpsc::sync_channel(1);
        player.receiver = Some(receiver);
        player.audio_clock = Some(Arc::new(AudioOutputState::new(48_000, 1.0)));
        player.current_frame = 100;
        player.total_frames = 200;

        player.seek_frame_instant(-10);

        assert_eq!(player.current_frame(), 90);
        assert!(player.receiver.is_none());
        assert!(player.audio_clock.is_none());
    }
}
