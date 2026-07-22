#![allow(clippy::items_after_test_module)]

#[path = "config.rs"]
mod implementation;

pub use implementation::*;

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};

pub const PLAYHEAD_OFFSET_MIN_PERCENT: f32 = -50.0;
pub const PLAYHEAD_OFFSET_MAX_PERCENT: f32 = 0.0;
pub const PLAYHEAD_OFFSET_STEP_PERCENT: f32 = 0.5;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(default)]
struct PlayheadOffsetConfig {
    offset_percent: f32,
}

impl Default for PlayheadOffsetConfig {
    fn default() -> Self {
        Self {
            offset_percent: 0.0,
        }
    }
}

static PLAYHEAD_OFFSET: OnceLock<RwLock<PlayheadOffsetConfig>> = OnceLock::new();

fn playhead_config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("coquerythmo")
        .join("playhead.toml")
}

fn normalize_playhead_offset(value: f32) -> f32 {
    let finite = if value.is_finite() { value } else { 0.0 };
    let stepped =
        (finite / PLAYHEAD_OFFSET_STEP_PERCENT).round() * PLAYHEAD_OFFSET_STEP_PERCENT;
    stepped.clamp(
        PLAYHEAD_OFFSET_MIN_PERCENT,
        PLAYHEAD_OFFSET_MAX_PERCENT,
    )
}

fn load_playhead_offset() -> PlayheadOffsetConfig {
    let path = playhead_config_path();
    let mut config = fs::read_to_string(&path)
        .ok()
        .and_then(|contents| toml::from_str::<PlayheadOffsetConfig>(&contents).ok())
        .unwrap_or_default();
    config.offset_percent = normalize_playhead_offset(config.offset_percent);
    config
}

fn playhead_offset_lock() -> &'static RwLock<PlayheadOffsetConfig> {
    PLAYHEAD_OFFSET.get_or_init(|| RwLock::new(load_playhead_offset()))
}

pub fn playhead_offset_percent() -> f32 {
    playhead_offset_lock()
        .read()
        .map(|config| config.offset_percent)
        .unwrap_or(0.0)
}

pub fn set_playhead_offset_percent(value: f32) {
    let value = normalize_playhead_offset(value);
    let Ok(mut config) = playhead_offset_lock().write() else {
        return;
    };
    if config.offset_percent == value {
        return;
    }
    config.offset_percent = value;

    let path = playhead_config_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    match toml::to_string_pretty(&*config) {
        Ok(contents) => {
            if let Err(error) = fs::write(path, contents) {
                log::warn!("Could not write playhead settings: {error}");
            }
        }
        Err(error) => log::warn!("Could not serialize playhead settings: {error}"),
    }
}

pub fn playhead_delta_pixels(width: f32) -> f32 {
    width.max(0.0) * playhead_offset_percent() / 100.0
}

pub fn playhead_x(zone_x: f32, zone_width: f32, playhead_width: f32) -> f32 {
    zone_x + (zone_width - playhead_width) * 0.5 + playhead_delta_pixels(zone_width)
}

#[cfg(test)]
mod playhead_offset_tests {
    use super::*;

    #[test]
    fn offset_is_quantized_and_clamped() {
        assert_eq!(normalize_playhead_offset(1.0), 0.0);
        assert_eq!(normalize_playhead_offset(-80.0), -50.0);
        assert_eq!(normalize_playhead_offset(-12.26), -12.5);
        assert_eq!(normalize_playhead_offset(-12.24), -12.0);
    }

    #[test]
    fn zero_is_center_and_minus_fifty_is_left() {
        let width = 800.0;
        let centered_x = width * (0.5 + normalize_playhead_offset(0.0) / 100.0);
        let left_x = width * (0.5 + normalize_playhead_offset(-50.0) / 100.0);
        assert_eq!(centered_x, 400.0);
        assert_eq!(left_x, 0.0);
    }
}
