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

pub const BADGE_HEIGHT: f32 = 14.0;
pub const BADGE_GAP: f32 = 2.0;
pub const BADGE_CHAR_W: f32 = 6.0;
pub const BADGE_FONT_SIZE: f32 = 9.0;

pub const SLOT_HEIGHT: f32 = 40.0;
pub const HANDLE_WIDTH: f32 = 6.0;
pub const RYTHMO_FONT_SIZE: f32 = 16.0;
