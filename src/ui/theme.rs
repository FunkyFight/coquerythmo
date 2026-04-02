/// Centralized theme — all colors, sizes, spacing.
/// No magic numbers outside this file.

// -- Zone backgrounds --
pub const TOPBAR_BG: [f32; 4] = [0.11, 0.11, 0.13, 1.0];
pub const TOPBAR_SHADOW: [f32; 4] = [0.20, 0.20, 0.24, 0.8];
pub const VIDEO_BG: [f32; 4] = [0.06, 0.06, 0.07, 1.0];
pub const TOOLBAR_BG: [f32; 4] = [0.10, 0.10, 0.12, 1.0];
pub const TOOLBAR_BORDER: [f32; 4] = [0.18, 0.18, 0.22, 0.6];
pub const RYTHMO_BG: [f32; 4] = [0.02, 0.02, 0.03, 1.0];
pub const PROPS_BG: [f32; 4] = [0.09, 0.09, 0.11, 1.0];
pub const PROPS_BORDER: [f32; 4] = [0.18, 0.18, 0.22, 0.6];

// -- Interactive widget states (buttons, dropdowns, icon buttons) --
pub const INTERACTIVE_BG_TOP_NORMAL: [f32; 4] = [0.20, 0.20, 0.23, 1.0];
pub const INTERACTIVE_BG_TOP_HOVERED: [f32; 4] = [0.26, 0.26, 0.30, 1.0];
pub const INTERACTIVE_BG_TOP_PRESSED: [f32; 4] = [0.12, 0.12, 0.14, 1.0];
pub const INTERACTIVE_BG_BOT_NORMAL: [f32; 4] = [0.13, 0.13, 0.15, 1.0];
pub const INTERACTIVE_BG_BOT_HOVERED: [f32; 4] = [0.18, 0.18, 0.21, 1.0];
pub const INTERACTIVE_BG_BOT_PRESSED: [f32; 4] = [0.08, 0.08, 0.10, 1.0];
pub const INTERACTIVE_BORDER_NORMAL: [f32; 4] = [0.30, 0.30, 0.36, 0.6];
pub const INTERACTIVE_BORDER_HOVERED: [f32; 4] = [0.45, 0.45, 0.55, 0.8];
pub const INTERACTIVE_BORDER_PRESSED: [f32; 4] = [0.20, 0.20, 0.25, 0.5];
pub const INTERACTIVE_SHADOW_NORMAL: [f32; 4] = [0.0, 0.0, 0.0, 0.4];
pub const INTERACTIVE_SHADOW_HOVERED: [f32; 4] = [0.0, 0.0, 0.0, 0.55];
pub const INTERACTIVE_SHADOW_PRESSED: [f32; 4] = [0.0, 0.0, 0.0, 0.2];

// -- Icon tints --
pub const ICON_TINT_NORMAL: [f32; 4] = [0.75, 0.75, 0.80, 1.0];
pub const ICON_TINT_HOVERED: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
pub const ICON_TINT_PRESSED: [f32; 4] = [0.55, 0.55, 0.60, 1.0];

// -- Transparent hover (icon buttons) --
pub const TRANSPARENT_HOVER: [f32; 4] = [1.0, 1.0, 1.0, 0.08];
pub const TRANSPARENT_PRESS: [f32; 4] = [1.0, 1.0, 1.0, 0.04];
pub const TRANSPARENT: [f32; 4] = [0.0, 0.0, 0.0, 0.0];

// -- Slider --
pub const SLIDER_TRACK_BG: [f32; 4] = [0.20, 0.20, 0.24, 1.0];
pub const SLIDER_TRACK_FILL: [f32; 4] = [0.35, 0.32, 0.75, 1.0];
pub const SLIDER_THUMB_NORMAL: [f32; 4] = [0.85, 0.85, 0.90, 1.0];
pub const SLIDER_THUMB_HOVER: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
pub const SLIDER_THUMB_PRESS: [f32; 4] = [0.65, 0.65, 0.70, 1.0];

// -- Rythmo --
pub const RYTHMO_TICK_COLOR: [f32; 4] = [0.40, 0.40, 0.45, 0.5];
pub const RYTHMO_PLAYHEAD: [f32; 4] = [0.85, 0.15, 0.15, 1.0];
pub const RYTHMO_PLAYHEAD_GLOW: [f32; 4] = [0.85, 0.15, 0.15, 0.3];
pub const RYTHMO_LINE_BORDER: [f32; 4] = [0.5, 0.5, 0.55, 0.3];
pub const RYTHMO_LINE_BORDER_HOVER: [f32; 4] = [0.6, 0.6, 0.65, 0.5];
pub const RYTHMO_LINE_BG_NORMAL: [f32; 4] = [0.08, 0.08, 0.10, 0.3];
pub const RYTHMO_LINE_BG_HOVER: [f32; 4] = [0.10, 0.10, 0.13, 0.4];
pub const RYTHMO_LINE_BG_EDIT: [f32; 4] = [0.12, 0.12, 0.15, 0.6];
pub const RYTHMO_HANDLE_COLOR: [f32; 4] = [0.9, 0.9, 0.95, 0.8];
pub const RYTHMO_CURSOR_COLOR: [f32; 4] = [0.9, 0.9, 0.95, 1.0];

// -- Tooltip --
pub const TOOLTIP_BG_TOP: [f32; 4] = [0.18, 0.18, 0.20, 0.95];
pub const TOOLTIP_BG_BOT: [f32; 4] = [0.14, 0.14, 0.16, 0.95];
pub const TOOLTIP_BORDER: [f32; 4] = [0.30, 0.30, 0.36, 0.6];

// -- Dropdown panel --
pub const DROPDOWN_PANEL_TOP: [f32; 4] = [0.15, 0.15, 0.17, 1.0];
pub const DROPDOWN_PANEL_BOT: [f32; 4] = [0.12, 0.12, 0.14, 1.0];
pub const DROPDOWN_PANEL_BORDER: [f32; 4] = [0.30, 0.30, 0.36, 0.6];
pub const DROPDOWN_HIGHLIGHT_HOVER: [f32; 4] = [1.0, 1.0, 1.0, 0.07];
pub const DROPDOWN_HIGHLIGHT_SELECTED: [f32; 4] = [0.30, 0.27, 0.75, 0.3];
pub const DROPDOWN_HIGHLIGHT_BOTH: [f32; 4] = [0.30, 0.27, 0.75, 0.5];

// -- Color picker --
pub const PICKER_BG_TOP: [f32; 4] = [0.12, 0.12, 0.14, 0.95];
pub const PICKER_BG_BOT: [f32; 4] = [0.10, 0.10, 0.12, 0.95];
pub const PICKER_BORDER: [f32; 4] = [0.3, 0.3, 0.36, 0.6];

// -- Text color --
pub const TEXT_COLOR: [u8; 3] = [224, 224, 224];

// -- Sizes --
pub const BORDER_RADIUS_DEFAULT: f32 = 8.0;
pub const BORDER_RADIUS_SMALL: f32 = 4.0;
pub const ICON_SIZE: u32 = 32;
pub const TOPBAR_HEIGHT: f32 = 32.0;
pub const TOOLBAR_HEIGHT: f32 = 40.0;
pub const TOOLBAR_BTN_SIZE: f32 = 32.0;
pub const SLIDER_TRACK_H: f32 = 4.0;
pub const SLIDER_THUMB_R: f32 = 7.0;
pub const SLIDER_W: f32 = 100.0;
pub const TOOLTIP_PADDING_H: f32 = 12.0;
pub const TOOLTIP_PADDING_V: f32 = 6.0;
pub const TOOLTIP_OFFSET_Y: f32 = 20.0;
pub const TOOLTIP_RADIUS: f32 = 4.0;

// -- Rythmo sizes --
pub const RYTHMO_TICK_WIDTH: f32 = 1.0;
pub const RYTHMO_TICK_LONG: f32 = 12.0;
pub const RYTHMO_TICK_SHORT: f32 = 6.0;
pub const RYTHMO_TICK_GAP: f32 = 8.0;
pub const RYTHMO_RULER_HEIGHT: f32 = 14.0;
pub const RYTHMO_PIXELS_PER_FRAME: f32 = 4.0;
pub const RYTHMO_NUM_SLOTS: f32 = 4.0;
pub const RYTHMO_HANDLE_WIDTH: f32 = 6.0;
pub const RYTHMO_PLAYHEAD_WIDTH: f32 = 2.0;
pub const RYTHMO_CURSOR_WIDTH: f32 = 1.5;

// -- Badge --
pub const BADGE_HEIGHT: f32 = 16.0;
pub const BADGE_PADDING_H: f32 = 6.0;
pub const BADGE_GAP: f32 = 2.0;
pub const BADGE_RADIUS: f32 = 2.0;
pub const BADGE_CHAR_W: f32 = 6.0;
pub const BADGE_MIN_W: f32 = 16.0;
pub const BADGE_FONT_SIZE: f32 = 10.0;

// -- Timing --
pub const DOUBLE_CLICK_MS: u128 = 400;
pub const CURSOR_BLINK_MS: u128 = 1000;
pub const CURSOR_BLINK_ON_MS: u128 = 500;
pub const SCROLL_MULTIPLIER: f32 = 30.0;
pub const DEFAULT_LINE_DURATION_SEC: f64 = 2.0;

// -- Audio --
pub const AUDIO_SAMPLE_RATE: u32 = 44100;
pub const AUDIO_CHANNELS: u16 = 2;
pub const VOLUME_MUTE_THRESHOLD: f32 = 0.01;
pub const VOLUME_DEFAULT: f32 = 0.75;
