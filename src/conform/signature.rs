//! Per-frame fingerprints of a video file, computed through ffmpeg.
//!
//! The picture of every frame is reduced to a 16×16 grey thumbnail and hashed
//! as a difference hash (64 bits: on every other row, each pixel compared to
//! its right-hand neighbour). The hash finds candidates quickly; the thumbnail
//! then tells identical pictures from merely similar ones. The sound is reduced to one 32-bit hash per frame: the sign of
//! the energy difference between neighbouring log-spaced bands, compared to
//! the previous frame (Haitsma–Kalker). Both survive re-encoding, grading and
//! resizing, which is what distinguishes two cuts of the same rushes.

use std::io::Read;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};

/// Thumbnail width and height; the hash compares 8 pixel pairs on 8 rows.
pub const THUMB_W: usize = 16;
pub const THUMB_H: usize = 16;
pub const THUMB_BYTES: usize = THUMB_W * THUMB_H;

const AUDIO_RATE: u32 = 8000;
const AUDIO_WINDOW: usize = 1024;
/// 33 band edges give 32 differences, one per hash bit.
const AUDIO_BANDS: usize = 33;
const AUDIO_LOW_HZ: f64 = 300.0;
const AUDIO_HIGH_HZ: f64 = 3000.0;

pub struct VideoSignature {
    /// Frames per second the hashes were sampled at.
    pub fps: f64,
    /// One difference hash per frame.
    pub hashes: Vec<u64>,
    /// `THUMB_BYTES` grey levels per frame, in the same order.
    pub thumbs: Vec<u8>,
    /// One audio hash per frame; empty when the file has no sound.
    pub audio: Vec<u32>,
}

impl VideoSignature {
    pub fn len(&self) -> i64 {
        self.hashes.len() as i64
    }

    pub fn is_empty(&self) -> bool {
        self.hashes.is_empty()
    }

    pub fn thumb(&self, frame: i64) -> Option<&[u8]> {
        if frame < 0 {
            return None;
        }
        let start = frame as usize * THUMB_BYTES;
        self.thumbs.get(start..start + THUMB_BYTES)
    }

    /// Sum of absolute grey differences between a frame of this signature
    /// and a frame of `other` (0 to 255 × [`THUMB_BYTES`]), or `None` when
    /// either is out of range.
    pub fn thumb_distance(&self, frame: i64, other: &VideoSignature, other_frame: i64) -> Option<u32> {
        let a = self.thumb(frame)?;
        let b = other.thumb(other_frame)?;
        Some(
            a.iter()
                .zip(b)
                .map(|(x, y)| (*x as i32 - *y as i32).unsigned_abs())
                .sum(),
        )
    }
}

/// Difference hash of one 16×16 grey thumbnail: 8 rows, 8 pairs each.
pub fn dhash(thumb: &[u8]) -> u64 {
    let mut hash = 0u64;
    let mut bit = 0;
    for row in (0..THUMB_H).step_by(2) {
        for pair in 0..8 {
            let left = thumb[row * THUMB_W + pair * 2];
            let right = thumb[row * THUMB_W + pair * 2 + 1];
            if left < right {
                hash |= 1 << bit;
            }
            bit += 1;
        }
    }
    hash
}

/// Decodes `path` and fingerprints every frame at `fps` frames per second.
///
/// `progress` receives a value in `0..=1`; `cancel` aborts the decode.
pub fn compute(
    path: &Path,
    fps: f64,
    duration_secs: f64,
    has_audio: bool,
    cancel: &AtomicBool,
    progress: &dyn Fn(f32),
) -> Result<VideoSignature, String> {
    if !fps.is_finite() || fps <= 0.0 {
        return Err("invalid frame rate".into());
    }
    let expected_frames = (duration_secs * fps).ceil().max(1.0);
    let video_share = if has_audio { 0.75 } else { 1.0 };
    let (hashes, thumbs) = decode_thumbs(path, fps, cancel, &|frames| {
        progress((frames as f64 / expected_frames).min(1.0) as f32 * video_share);
    })?;
    let audio = if has_audio {
        let samples = decode_audio(path, cancel)?;
        progress(0.9);
        audio_hashes(&samples, AUDIO_RATE, fps, hashes.len())
    } else {
        Vec::new()
    };
    progress(1.0);
    Ok(VideoSignature {
        fps,
        hashes,
        thumbs,
        audio,
    })
}

fn decode_thumbs(
    path: &Path,
    fps: f64,
    cancel: &AtomicBool,
    progress: &dyn Fn(usize),
) -> Result<(Vec<u64>, Vec<u8>), String> {
    let filter = format!(
        "fps={fps:.6},scale={THUMB_W}:{THUMB_H}:flags=area,format=gray"
    );
    let mut child = crate::media_binary::command("ffmpeg")
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(path)
        .args(["-an", "-sn", "-vf", &filter, "-f", "rawvideo", "-pix_fmt", "gray", "pipe:1"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("ffmpeg: {error}"))?;
    let stdout = child.stdout.take().expect("piped stdout");
    let mut reader = std::io::BufReader::with_capacity(1 << 16, stdout);
    let mut hashes = Vec::new();
    let mut thumbs = Vec::new();
    let mut frame = [0u8; THUMB_BYTES];
    loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("cancelled".into());
        }
        match read_full(&mut reader, &mut frame) {
            Ok(true) => {
                hashes.push(dhash(&frame));
                thumbs.extend_from_slice(&frame);
                if hashes.len() % 256 == 0 {
                    progress(hashes.len());
                }
            }
            Ok(false) => break,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("ffmpeg read: {error}"));
            }
        }
    }
    let status = child.wait().map_err(|error| format!("ffmpeg: {error}"))?;
    if hashes.is_empty() {
        let mut stderr = String::new();
        if let Some(mut pipe) = child.stderr.take() {
            let _ = pipe.read_to_string(&mut stderr);
        }
        return Err(if status.success() {
            "no video frame decoded".to_string()
        } else {
            format!("ffmpeg failed: {}", stderr.trim())
        });
    }
    Ok((hashes, thumbs))
}

/// Fills `buffer` entirely. `Ok(false)` means a clean end of stream before
/// any byte of this frame.
fn read_full(reader: &mut impl Read, buffer: &mut [u8]) -> std::io::Result<bool> {
    let mut filled = 0;
    while filled < buffer.len() {
        match reader.read(&mut buffer[filled..]) {
            Ok(0) if filled == 0 => return Ok(false),
            Ok(0) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "truncated frame",
                ))
            }
            Ok(read) => filled += read,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    Ok(true)
}

fn decode_audio(path: &Path, cancel: &AtomicBool) -> Result<Vec<f32>, String> {
    let mut child = crate::media_binary::command("ffmpeg")
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(path)
        .args([
            "-vn",
            "-sn",
            "-af",
            "aresample=async=1:first_pts=0",
            "-f",
            "f32le",
            "-acodec",
            "pcm_f32le",
            "-ar",
            &AUDIO_RATE.to_string(),
            "-ac",
            "1",
            "pipe:1",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("ffmpeg audio: {error}"))?;
    let stdout = child.stdout.take().expect("piped stdout");
    let mut reader = std::io::BufReader::with_capacity(1 << 16, stdout);
    let mut samples = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("cancelled".into());
        }
        match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => {
                let usable = read - read % 4;
                samples.extend(
                    chunk[..usable]
                        .chunks_exact(4)
                        .map(|bytes| f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])),
                );
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("ffmpeg audio read: {error}"));
            }
        }
    }
    let _ = child.wait();
    Ok(samples)
}

/// One 32-bit hash per video frame from mono PCM at `rate` Hz.
pub fn audio_hashes(samples: &[f32], rate: u32, fps: f64, frame_count: usize) -> Vec<u32> {
    if samples.is_empty() || frame_count == 0 {
        return Vec::new();
    }
    let edges = band_edges(rate);
    let window = hann_window();
    let mut previous: Option<[f32; AUDIO_BANDS - 1]> = None;
    let mut hashes = Vec::with_capacity(frame_count);
    let mut spectrum = vec![(0.0f32, 0.0f32); AUDIO_WINDOW];
    for frame in 0..frame_count {
        let centre = ((frame as f64 + 0.5) / fps * rate as f64) as i64;
        let start = centre - AUDIO_WINDOW as i64 / 2;
        for (index, slot) in spectrum.iter_mut().enumerate() {
            let position = start + index as i64;
            let sample = if position >= 0 && (position as usize) < samples.len() {
                samples[position as usize]
            } else {
                0.0
            };
            *slot = (sample * window[index], 0.0);
        }
        fft_in_place(&mut spectrum);
        let mut energies = [0.0f32; AUDIO_BANDS - 1];
        for (band, energy) in energies.iter_mut().enumerate() {
            let sum: f32 = spectrum[edges[band]..edges[band + 1]]
                .iter()
                .map(|(re, im)| re * re + im * im)
                .sum();
            *energy = (sum + 1e-9).ln();
        }
        let mut hash = 0u32;
        if let Some(last) = previous {
            for bit in 0..AUDIO_BANDS - 2 {
                let now = energies[bit] - energies[bit + 1];
                let before = last[bit] - last[bit + 1];
                if now - before > 0.0 {
                    hash |= 1 << bit;
                }
            }
        }
        hashes.push(hash);
        previous = Some(energies);
    }
    hashes
}

fn band_edges(rate: u32) -> [usize; AUDIO_BANDS] {
    let bin_hz = rate as f64 / AUDIO_WINDOW as f64;
    let ratio = (AUDIO_HIGH_HZ / AUDIO_LOW_HZ).ln() / (AUDIO_BANDS - 1) as f64;
    let mut edges = [0usize; AUDIO_BANDS];
    for (index, edge) in edges.iter_mut().enumerate() {
        let hz = AUDIO_LOW_HZ * (ratio * index as f64).exp();
        *edge = ((hz / bin_hz).round() as usize).min(AUDIO_WINDOW / 2 - 1);
    }
    for index in 1..AUDIO_BANDS {
        if edges[index] <= edges[index - 1] {
            edges[index] = edges[index - 1] + 1;
        }
    }
    edges
}

fn hann_window() -> Vec<f32> {
    (0..AUDIO_WINDOW)
        .map(|index| {
            let phase = 2.0 * std::f64::consts::PI * index as f64 / AUDIO_WINDOW as f64;
            (0.5 - 0.5 * phase.cos()) as f32
        })
        .collect()
}

/// Iterative radix-2 FFT; `data.len()` must be a power of two.
fn fft_in_place(data: &mut [(f32, f32)]) {
    let n = data.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            data.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let angle = -2.0 * std::f32::consts::PI / len as f32;
        let (step_sin, step_cos) = angle.sin_cos();
        for start in (0..n).step_by(len) {
            let (mut w_re, mut w_im) = (1.0f32, 0.0f32);
            for k in 0..len / 2 {
                let (a_re, a_im) = data[start + k];
                let (b_re, b_im) = data[start + k + len / 2];
                let t_re = b_re * w_re - b_im * w_im;
                let t_im = b_re * w_im + b_im * w_re;
                data[start + k] = (a_re + t_re, a_im + t_im);
                data[start + k + len / 2] = (a_re - t_re, a_im - t_im);
                let next_re = w_re * step_cos - w_im * step_sin;
                w_im = w_re * step_sin + w_im * step_cos;
                w_re = next_re;
            }
        }
        len <<= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dhash_compares_horizontal_neighbours() {
        let mut thumb = [0u8; THUMB_BYTES];
        // First row rising, every other row flat.
        for column in 0..THUMB_W {
            thumb[column] = column as u8 * 10;
        }
        assert_eq!(dhash(&thumb), 0xff);
        // The second row is skipped: only even rows are hashed.
        for column in 0..THUMB_W {
            thumb[THUMB_W + column] = column as u8 * 10;
        }
        assert_eq!(dhash(&thumb), 0xff);
        let flat = [7u8; THUMB_BYTES];
        assert_eq!(dhash(&flat), 0);
    }

    #[test]
    fn fft_finds_a_pure_tone() {
        let mut data: Vec<(f32, f32)> = (0..AUDIO_WINDOW)
            .map(|i| {
                let phase = 2.0 * std::f32::consts::PI * 64.0 * i as f32 / AUDIO_WINDOW as f32;
                (phase.sin(), 0.0)
            })
            .collect();
        fft_in_place(&mut data);
        let magnitudes: Vec<f32> = data
            .iter()
            .take(AUDIO_WINDOW / 2)
            .map(|(re, im)| (re * re + im * im).sqrt())
            .collect();
        let peak = magnitudes
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(index, _)| index)
            .unwrap();
        assert_eq!(peak, 64);
    }

    #[test]
    fn audio_hashes_are_stable_under_small_noise_and_change_with_content() {
        let rate = AUDIO_RATE;
        let fps = 25.0;
        let seconds = 2.0;
        let count = (rate as f64 * seconds) as usize;
        let tone = |hz: f32, i: usize| {
            (2.0 * std::f32::consts::PI * hz * i as f32 / rate as f32).sin()
        };
        let original: Vec<f32> = (0..count)
            .map(|i| 0.5 * tone(440.0, i) + 0.3 * tone(1200.0 + (i / 800) as f32 * 50.0, i))
            .collect();
        let noisy: Vec<f32> = original
            .iter()
            .enumerate()
            .map(|(i, s)| s * 0.8 + 0.01 * tone(2500.0, i))
            .collect();
        let other: Vec<f32> = (0..count)
            .map(|i| 0.5 * tone(900.0 + (i / 500) as f32 * 80.0, i))
            .collect();
        let frames = (seconds * fps) as usize;
        let a = audio_hashes(&original, rate, fps, frames);
        let b = audio_hashes(&noisy, rate, fps, frames);
        let c = audio_hashes(&other, rate, fps, frames);
        let distance = |x: &[u32], y: &[u32]| -> u32 {
            x.iter().zip(y).map(|(p, q)| (p ^ q).count_ones()).sum::<u32>()
        };
        assert!(distance(&a, &b) * 4 < distance(&a, &c));
    }
}
