use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use rodio::{OutputStream, OutputStreamHandle, Sink, Source};

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
    last_frame_time: Option<Instant>,
    receiver: Option<Receiver<VideoFrame>>,
    decoder_handle: Option<JoinHandle<()>>,
    path: Option<PathBuf>,
    finished: bool,
    current_frame: i64,
    total_frames: i64,
    volume: f32,

    _audio_stream: Option<OutputStream>,
    _audio_handle: Option<OutputStreamHandle>,
    audio_sink: Option<Sink>,
    audio_thread: Option<JoinHandle<()>>,
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
            last_frame_time: None,
            receiver: None,
            decoder_handle: None,
            path: None,
            finished: false,
            current_frame: 0,
            total_frames: 0,
            volume: 0.75,
            _audio_stream: None,
            _audio_handle: None,
            audio_sink: None,
            audio_thread: None,
        }
    }

    pub fn load(
        &mut self,
        path: &Path,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bind_group_layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
    ) -> Result<(), String> {
        self.stop();

        let (width, height, fps, total_frames) = probe_video(path)?;
        self.width = width;
        self.height = height;
        self.fps = fps;
        self.total_frames = total_frames;
        self.current_frame = 0;
        self.path = Some(path.to_path_buf());
        self.finished = false;

        log::info!("Video: {}x{} @ {:.2} fps, {} frames from {}", width, height, fps, total_frames, path.display());

        let first_frame = decode_frame_at(path, width, height, 0.0)?;
        self.upload_frame(&first_frame, device, queue, bind_group_layout, sampler);

        self.start_decoders_at(0.0);

        Ok(())
    }

    pub fn set_volume(&mut self, vol: f32) {
        self.volume = vol;
        if let Some(sink) = &self.audio_sink {
            let v = if vol < 0.01 { 0.0 } else { vol };
            sink.set_volume(v);
        }
    }

    pub fn toggle(&mut self) {
        if self.finished {
            return;
        }
        self.playing = !self.playing;
        if self.playing {
            self.last_frame_time = None;
            // Always resync decoders to current position on play
            let ts = self.current_frame as f64 / self.fps;
            self.stop_decoders();
            self.start_decoders_at(ts);
            if let Some(sink) = &self.audio_sink {
                sink.play();
            }
        } else {
            if let Some(sink) = &self.audio_sink {
                sink.pause();
            }
        }
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// Move current_frame by delta WITHOUT decoding (instant, for scroll).
    pub fn seek_frame_instant(&mut self, delta: i32) {
        let target = (self.current_frame + delta as i64).max(0);
        let target = if self.total_frames > 0 {
            target.min(self.total_frames - 1)
        } else {
            target
        };
        self.current_frame = target;
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

        let frame_duration = Duration::from_secs_f64(1.0 / self.fps);
        let now = Instant::now();

        let should_advance = match self.last_frame_time {
            Some(last) => now.duration_since(last) >= frame_duration,
            None => true,
        };

        if !should_advance {
            return;
        }

        let recv_frame = self.receiver.as_ref().and_then(|rx| rx.try_recv().ok());
        if let Some(frame) = recv_frame {
            self.upload_frame(&frame, device, queue, bind_group_layout, sampler);
            self.last_frame_time = Some(now);
            self.current_frame += 1;
        } else {
            let disconnected = self.receiver.as_ref().map_or(false, |rx| {
                matches!(rx.try_recv(), Err(mpsc::TryRecvError::Disconnected))
            });
            if disconnected {
                self.playing = false;
                self.finished = true;
                self.receiver = None;
                if let Some(sink) = &self.audio_sink {
                    sink.pause();
                }
                log::info!("Video playback finished");
            }
        }
    }

    pub fn current_frame(&self) -> i64 {
        self.current_frame
    }

    pub fn fps(&self) -> f64 {
        self.fps
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

        // Video decoder
        let (tx, rx) = mpsc::sync_channel::<VideoFrame>(3);
        let path_clone = path.clone();
        let w = self.width;
        let h = self.height;
        let vid_handle = thread::spawn(move || {
            decode_video_stream_from(path_clone, w, h, timestamp, tx);
        });
        self.receiver = Some(rx);
        self.decoder_handle = Some(vid_handle);

        // Audio decoder
        if self.setup_audio_from(&path, timestamp).is_ok() {
            self.set_volume(self.volume);
            if !self.playing {
                if let Some(sink) = &self.audio_sink {
                    sink.pause();
                }
            }
        }
    }

    fn setup_audio_from(&mut self, path: &Path, timestamp: f64) -> Result<(), String> {
        let (stream, handle) = OutputStream::try_default()
            .map_err(|e| format!("Audio output failed: {e}"))?;
        let sink = Sink::try_new(&handle)
            .map_err(|e| format!("Audio sink failed: {e}"))?;
        sink.pause();

        let (audio_tx, audio_rx) = mpsc::sync_channel::<Vec<f32>>(8);
        let path_clone = path.to_path_buf();
        let audio_handle = thread::spawn(move || {
            decode_audio_stream_from(path_clone, timestamp, audio_tx);
        });

        let source = AudioChannelSource::new(audio_rx);
        sink.append(source);

        self._audio_stream = Some(stream);
        self._audio_handle = Some(handle);
        self.audio_sink = Some(sink);
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
                    width: frame.width, height: frame.height, depth_or_array_layers: 1,
                },
                mip_level_count: 1, sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Video BG"), layout: bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
                ],
            });
            self.texture = Some(texture);
            self.texture_view = Some(view);
            self.bind_group = Some(bind_group);
        }

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: self.texture.as_ref().unwrap(),
                mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All,
            },
            &frame.data,
            wgpu::TexelCopyBufferLayout {
                offset: 0, bytes_per_row: Some(4 * frame.width), rows_per_image: Some(frame.height),
            },
            wgpu::Extent3d { width: frame.width, height: frame.height, depth_or_array_layers: 1 },
        );
    }

    fn stop_decoders(&mut self) {
        if let Some(sink) = &self.audio_sink {
            sink.stop();
        }
        self.audio_sink = None;
        self._audio_handle = None;
        self._audio_stream = None;
        self.receiver = None;
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

// --- Audio channel source for rodio ---

const AUDIO_SAMPLE_RATE: u32 = 44100;
const AUDIO_CHANNELS: u16 = 2;

struct AudioChannelSource {
    rx: Receiver<Vec<f32>>,
    buffer: Vec<f32>,
    pos: usize,
}

impl AudioChannelSource {
    fn new(rx: Receiver<Vec<f32>>) -> Self {
        Self { rx, buffer: Vec::new(), pos: 0 }
    }
}

impl Iterator for AudioChannelSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        loop {
            if self.pos < self.buffer.len() {
                let sample = self.buffer[self.pos];
                self.pos += 1;
                return Some(sample);
            }
            match self.rx.recv() {
                Ok(chunk) => { self.buffer = chunk; self.pos = 0; }
                Err(_) => return None,
            }
        }
    }
}

impl Source for AudioChannelSource {
    fn current_frame_len(&self) -> Option<usize> { None }
    fn channels(&self) -> u16 { AUDIO_CHANNELS }
    fn sample_rate(&self) -> u32 { AUDIO_SAMPLE_RATE }
    fn total_duration(&self) -> Option<Duration> { None }
}

// --- ffmpeg subprocess functions ---

fn probe_video(path: &Path) -> Result<(u32, u32, f64, i64), String> {
    let output = Command::new("ffprobe")
        .args([
            "-v", "error", "-select_streams", "v:0",
            "-show_entries", "stream=width,height,r_frame_rate,nb_frames",
            "-of", "csv=p=0:s=,",
        ])
        .arg(path)
        .output()
        .map_err(|e| format!("ffprobe failed: {e}"))?;

    if !output.status.success() {
        return Err(format!("ffprobe error: {}", String::from_utf8_lossy(&output.stderr)));
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let text = text.trim();
    let parts: Vec<&str> = text.split(',').collect();
    if parts.len() < 3 {
        return Err(format!("Unexpected ffprobe output: {text}"));
    }

    let width: u32 = parts[0].parse().map_err(|e| format!("Bad width: {e}"))?;
    let height: u32 = parts[1].parse().map_err(|e| format!("Bad height: {e}"))?;
    let fps = parse_frame_rate(parts[2])?;
    let total_frames: i64 = if parts.len() > 3 { parts[3].parse().unwrap_or(0) } else { 0 };

    Ok((width, height, fps, total_frames))
}

fn parse_frame_rate(s: &str) -> Result<f64, String> {
    let parts: Vec<&str> = s.split('/').collect();
    if parts.len() == 2 {
        let num: f64 = parts[0].parse().map_err(|e| format!("Bad fps num: {e}"))?;
        let den: f64 = parts[1].parse().map_err(|e| format!("Bad fps den: {e}"))?;
        if den > 0.0 { Ok(num / den) } else { Ok(30.0) }
    } else {
        s.parse::<f64>().map_err(|e| format!("Bad fps: {e}"))
    }
}

fn decode_frame_at(path: &Path, width: u32, height: u32, timestamp: f64) -> Result<VideoFrame, String> {
    let ts = format!("{:.6}", timestamp);
    let output = Command::new("ffmpeg")
        .args(["-ss", &ts])
        .arg("-i").arg(path)
        .args(["-frames:v", "1", "-pix_fmt", "rgba", "-f", "rawvideo", "-v", "error", "pipe:1"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("ffmpeg seek failed: {e}"))?;

    if !output.status.success() {
        return Err(format!("ffmpeg error: {}", String::from_utf8_lossy(&output.stderr)));
    }

    let expected = (width * height * 4) as usize;
    if output.stdout.len() != expected {
        return Err(format!("Bad frame size: {} vs expected {}", output.stdout.len(), expected));
    }

    Ok(VideoFrame { data: output.stdout, width, height })
}

fn decode_video_stream_from(path: PathBuf, width: u32, height: u32, timestamp: f64, tx: SyncSender<VideoFrame>) {
    let ts = format!("{:.6}", timestamp);
    let mut child = match Command::new("ffmpeg")
        .args(["-ss", &ts])
        .arg("-i").arg(&path)
        .args(["-pix_fmt", "rgba", "-f", "rawvideo", "-v", "error", "pipe:1"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => { log::error!("Failed to spawn ffmpeg video decoder: {e}"); return; }
    };

    let stdout = child.stdout.take().unwrap();
    let mut reader = std::io::BufReader::with_capacity(width as usize * height as usize * 4, stdout);
    let frame_size = (width * height * 4) as usize;
    let mut buf = vec![0u8; frame_size];

    // Skip first frame (already shown by decode_frame_at)
    if reader.read_exact(&mut buf).is_err() {
        let _ = child.wait();
        return;
    }

    loop {
        match reader.read_exact(&mut buf) {
            Ok(()) => {
                let frame = VideoFrame { data: buf.clone(), width, height };
                if tx.send(frame).is_err() { break; }
            }
            Err(_) => break,
        }
    }

    let _ = child.wait();
}

fn decode_audio_stream_from(path: PathBuf, timestamp: f64, tx: SyncSender<Vec<f32>>) {
    let ts = format!("{:.6}", timestamp);
    let mut child = match Command::new("ffmpeg")
        .args(["-ss", &ts])
        .arg("-i").arg(&path)
        .args([
            "-vn", "-f", "f32le", "-acodec", "pcm_f32le",
            "-ar", &AUDIO_SAMPLE_RATE.to_string(),
            "-ac", &AUDIO_CHANNELS.to_string(),
            "-v", "error", "pipe:1",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => { log::error!("Failed to spawn ffmpeg audio decoder: {e}"); return; }
    };

    let stdout = child.stdout.take().unwrap();
    let mut reader = std::io::BufReader::new(stdout);
    let chunk_samples = 4096 * AUDIO_CHANNELS as usize;
    let chunk_bytes = chunk_samples * 4;
    let mut buf = vec![0u8; chunk_bytes];

    loop {
        match reader.read_exact(&mut buf) {
            Ok(()) => {
                let samples: Vec<f32> = buf
                    .chunks_exact(4)
                    .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                    .collect();
                if tx.send(samples).is_err() { break; }
            }
            Err(_) => break,
        }
    }

    let _ = child.wait();
}
