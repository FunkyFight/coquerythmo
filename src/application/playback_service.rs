//! Playback-owned state. The player remains the existing media adapter; this
//! component owns the playback session values that coordinate it with the UI.

use std::path::PathBuf;
use std::time::Instant;

use crate::observer::TimelineBus;
use crate::video::VideoPlayer;

pub struct PlaybackSession {
    pub video_player: Option<VideoPlayer>,
    pub source_video_path: Option<PathBuf>,
    pub source_video_size: Option<(u32, u32)>,
    pub proxy_video_path: Option<PathBuf>,
    pub timeline: TimelineBus,
    pub last_scroll_time: Option<Instant>,
    pub scroll_needs_decode: bool,
    pub last_waveform_revision: u64,
    pub last_nonzero_volume: f32,
}

impl PlaybackSession {
    pub fn new() -> Self {
        Self {
            video_player: None,
            source_video_path: None,
            source_video_size: None,
            proxy_video_path: None,
            timeline: TimelineBus::new(),
            last_scroll_time: None,
            scroll_needs_decode: false,
            last_waveform_revision: 0,
            last_nonzero_volume: 0.75,
        }
    }
}

impl Default for PlaybackSession {
    fn default() -> Self {
        Self::new()
    }
}
