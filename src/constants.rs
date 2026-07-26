/// JavaScript `Number.MAX_SAFE_INTEGER` — used for ID generation
/// to ensure compatibility with the Node.js collaboration server.
pub const JS_MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

/// Y-axis track positions for rythmo lines.
pub const Y_SLOTS: [f32; 4] = [0.25, 0.5, 0.75, 1.0];
pub const NUM_SLOTS: f32 = 4.0;

/// Pixels per frame at reference width (shared between UI and CPU renderer).
pub const PIXELS_PER_FRAME: f32 = 6.0;

/// Reference width used for scaling in the CPU renderer.
pub const REF_WIDTH: f32 = 800.0;

/// Default export framerate.
///
/// Export rendering uses an explicit output framerate and remains independent
/// from the interactive display cadence managed by `frame_timing`.
pub const DEFAULT_EXPORT_FPS: u32 = 60;

/// Delay (ms) before decoding a frame after scrolling stops.
pub const SCROLL_DECODE_DELAY_MS: u128 = 100;

/// Default line duration in seconds when creating a new line.
pub const DEFAULT_LINE_DURATION_SEC: f64 = 2.0;

// -- Rythmo rendering constants (shared between GPU UI and CPU export) --

pub const RULER_HEIGHT: f32 = 28.0;
pub const TICK_LONG: f32 = 12.0;
pub const TICK_SHORT: f32 = 6.0;
pub const TICK_GAP_FRAMES: i64 = 2;

pub const BADGE_HEIGHT: f32 = 13.0;
pub const BADGE_GAP: f32 = 2.0;
pub const BADGE_CHAR_W: f32 = 7.5;
pub const BADGE_FONT_SIZE: f32 = 11.0;
pub const BADGE_OVERLAP_HEIGHT_RATIO: f32 = 0.45;
pub const VOICE_ACTOR_DISPLAY_ICON_SIZE: f32 = 28.0;

pub const SLOT_HEIGHT: f32 = 40.0;
pub const HANDLE_WIDTH: f32 = 6.0;
pub const RYTHMO_FONT_SIZE: f32 = 16.0;
pub const CHARACTER_LABEL_FONT_SIZE: f32 = SLOT_HEIGHT / 1.4;

pub const KARAOKE_DOT_SIZE: f32 = 7.0;
pub const KARAOKE_DOT_BOUNCE_AMPLITUDE: f32 = 1.45;
pub const KARAOKE_NEXT_PREVIEW_GAP: f32 = 8.0;
pub const KARAOKE_TEXT_FONT_SCALE: f32 = 1.20;
pub const KARAOKE_ADJACENT_MAX_GAP_SECONDS: f64 = 30.0;
pub const KARAOKE_COUNT_IN_SECONDS: f64 = 1.5;
pub const KARAOKE_COUNT_IN_BOUNCES: f32 = 3.0;
pub const CHARACTER_BADGE_COLLISION_OPACITY: f32 = 0.6;
