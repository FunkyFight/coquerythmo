//! Interactive-only audio adapter for the 1000 Hz production marker.

use crate::project::Project;
use crate::rythmo_special_markers::{markers, SpecialMarkerKind};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

const TONE_HZ: f32 = 1000.0;
const TONE_SECONDS: f64 = 0.100;
const TONE_AMPLITUDE: f32 = 0.78;
static TONE_GENERATION: AtomicU64 = AtomicU64::new(0);

#[derive(Default)]
struct PlaybackTracker {
    last_frame: Option<f64>,
    was_playing: bool,
}

fn tracker() -> &'static Mutex<PlaybackTracker> {
    static TRACKER: OnceLock<Mutex<PlaybackTracker>> = OnceLock::new();
    TRACKER.get_or_init(|| Mutex::new(PlaybackTracker::default()))
}

fn lock_tracker() -> std::sync::MutexGuard<'static, PlaybackTracker> {
    tracker()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn crossed_marker(previous: f64, current: f64, marker: f64, started: bool) -> bool {
    if current < previous {
        return false;
    }
    (marker > previous && marker <= current)
        || (started && (marker - current).abs() <= 0.5)
}

pub fn sync_playback(project: &Project, current_frame: f64, playing: bool) {
    let mut tracker = lock_tracker();
    let previous = tracker.last_frame.unwrap_or(current_frame);
    let started = playing && !tracker.was_playing;
    tracker.last_frame = Some(current_frame);
    tracker.was_playing = playing;
    drop(tracker);

    if !playing {
        return;
    }

    let should_play = markers(project).into_iter().any(|marker| {
        marker.kind == SpecialMarkerKind::Bip1000
            && crossed_marker(
                previous,
                current_frame,
                marker.media_tick.as_frame_position(),
                started,
            )
    });
    if should_play {
        play_tone_async();
    }
}

pub fn reset() {
    *lock_tracker() = PlaybackTracker::default();
    TONE_GENERATION.fetch_add(1, Ordering::SeqCst);
}

fn play_tone_async() {
    let generation = TONE_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    std::thread::spawn(move || {
        if let Err(error) = play_tone(generation) {
            log::warn!("1000 Hz marker playback failed: {error}");
        }
    });
}

fn play_tone(generation: u64) -> Result<(), String> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| "aucune sortie audio disponible".to_string())?;
    let supported = device
        .default_output_config()
        .map_err(|error| format!("configuration audio indisponible: {error}"))?;
    let config = supported.config();
    let sample_rate = config.sample_rate.0;
    let channels = config.channels as usize;
    let frames = (TONE_SECONDS * sample_rate as f64).ceil() as usize;
    let mut samples = vec![0.0_f32; frames.saturating_mul(channels)];
    fill_tone(&mut samples, channels, sample_rate);

    let shared = Arc::new(Mutex::new((samples, 0usize)));
    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => build_stream::<f32>(&device, &config, shared, generation),
        cpal::SampleFormat::I16 => build_stream::<i16>(&device, &config, shared, generation),
        cpal::SampleFormat::U16 => build_stream::<u16>(&device, &config, shared, generation),
        format => return Err(format!("format audio non pris en charge: {format:?}")),
    }?;
    stream
        .play()
        .map_err(|error| format!("lecture audio impossible: {error}"))?;
    std::thread::sleep(Duration::from_secs_f64(TONE_SECONDS + 0.03));
    drop(stream);
    Ok(())
}

fn fill_tone(samples: &mut [f32], channels: usize, sample_rate: u32) {
    if channels == 0 || sample_rate == 0 {
        return;
    }
    let frames = samples.len() / channels;
    for frame in 0..frames {
        let progress = frame as f32 / frames.max(1) as f32;
        let attack = (progress / 0.08).min(1.0);
        let release = ((1.0 - progress) / 0.12).min(1.0);
        let envelope = attack.min(release);
        let phase = std::f32::consts::TAU * TONE_HZ * frame as f32 / sample_rate as f32;
        let value = phase.sin() * TONE_AMPLITUDE * envelope;
        for channel in 0..channels {
            samples[frame * channels + channel] = value;
        }
    }
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    shared: Arc<Mutex<(Vec<f32>, usize)>>,
    generation: u64,
) -> Result<cpal::Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    device
        .build_output_stream(
            config,
            move |output: &mut [T], _| {
                let active = TONE_GENERATION.load(Ordering::SeqCst) == generation;
                let Ok(mut guard) = shared.lock() else {
                    for sample in output {
                        *sample = T::from_sample(0.0);
                    }
                    return;
                };
                let (samples, cursor) = &mut *guard;
                for output_sample in output {
                    let value = if active {
                        samples.get(*cursor).copied().unwrap_or(0.0)
                    } else {
                        0.0
                    };
                    *output_sample = T::from_sample(value);
                    *cursor = cursor.saturating_add(1);
                }
            },
            move |error| log::warn!("1000 Hz marker stream error: {error}"),
            None,
        )
        .map_err(|error| format!("création du flux audio impossible: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_frequency_fills_a_bounded_waveform() {
        let sample_rate = 48_000;
        let mut samples = vec![0.0; 4_800 * 2];
        fill_tone(&mut samples, 2, sample_rate);
        let peak = samples
            .iter()
            .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
        assert!(peak > 0.7);
        assert!(peak <= TONE_AMPLITUDE);
    }

    #[test]
    fn marker_crossing_is_forward_only() {
        assert!(crossed_marker(9.0, 10.0, 10.0, false));
        assert!(!crossed_marker(10.0, 9.0, 9.5, false));
        assert!(crossed_marker(10.0, 10.0, 10.0, true));
    }
}
