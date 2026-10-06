//! Second step of the video export: placement of the video and of the
//! rythmo band on the exported frames, with a live preview of the frame at
//! the playhead (real video frame and band rendered by the export renderer).

use super::primitives::{HAlign, LabelInfo, Overflow, QuadInstance, Rect, VAlign};
use crate::export_layout::{self, ComposedLayout, ExportLayout, LayerTransform, PixelRect};
use crate::i18n::t;
use crate::project::ExportConfiguration;

/// X, Y, width and height for the video (0..4) then the band (4..8).
pub const STEPPER_COUNT: usize = 8;
pub const BR_SCALE_CONTROL: usize = 8;
pub const KARAOKE_SCALE_CONTROL: usize = 9;
/// Controls that hold a value (adjusted with the arrows).
pub const VALUE_COUNT: usize = 10;
pub const RESET_CONTROL: usize = 10;
pub const BACK_CONTROL: usize = 11;
pub const LAUNCH_CONTROL: usize = 12;
pub const CONTROL_COUNT: usize = 13;

const SIDEBAR_W: f32 = 340.0;
const SIDEBAR_PAD: f32 = 20.0;
const FIRST_SECTION_Y: f32 = 92.0;
const HEADER_H: f32 = 26.0;
const SECTION_GAP: f32 = 10.0;
const BUTTON_H: f32 = 40.0;
const OFFSET_STEP: f32 = 0.01;
const SCALE_STEP: f32 = 0.05;
const BR_SCALE_STEP: f32 = 0.1;
/// Widest band texture rendered for the preview (largest GPU texture side).
pub const MAX_PREVIEW_BAND_WIDTH: u32 = 8192;
/// The preview band width is rounded up to a multiple of this, so resizing
/// the band does not render it again on every pixel.
pub const PREVIEW_BAND_WIDTH_STEP: u32 = 64;
/// Side of the resize handles drawn on the selected layer.
const HANDLE_SIZE: f32 = 10.0;
/// Distance from an edge at which the pointer grabs it.
const HANDLE_GRAB: f32 = 7.0;
/// Changes kept by the layout's undo history.
const HISTORY_LIMIT: usize = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    Video,
    Band,
}

/// Resize handle of a layer in the preview: an edge or a corner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    Left,
    Right,
    Top,
    Bottom,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Handle {
    pub const ALL: [Handle; 8] = [
        Handle::TopLeft,
        Handle::Top,
        Handle::TopRight,
        Handle::Right,
        Handle::BottomRight,
        Handle::Bottom,
        Handle::BottomLeft,
        Handle::Left,
    ];

    fn left(self) -> bool {
        matches!(self, Self::Left | Self::TopLeft | Self::BottomLeft)
    }

    fn right(self) -> bool {
        matches!(self, Self::Right | Self::TopRight | Self::BottomRight)
    }

    fn top(self) -> bool {
        matches!(self, Self::Top | Self::TopLeft | Self::TopRight)
    }

    fn bottom(self) -> bool {
        matches!(self, Self::Bottom | Self::BottomLeft | Self::BottomRight)
    }

    /// Centre of the handle on a layer's on-screen rect.
    fn anchor(self, rect: Rect) -> (f32, f32) {
        let x = if self.left() {
            rect.x
        } else if self.right() {
            rect.x + rect.width
        } else {
            rect.x + rect.width / 2.0
        };
        let y = if self.top() {
            rect.y
        } else if self.bottom() {
            rect.y + rect.height
        } else {
            rect.y + rect.height / 2.0
        };
        (x, y)
    }

    pub fn cursor(self) -> LayoutCursor {
        match self {
            Self::Left | Self::Right => LayoutCursor::ResizeHorizontal,
            Self::Top | Self::Bottom => LayoutCursor::ResizeVertical,
            Self::TopLeft | Self::BottomRight => LayoutCursor::ResizeDiagonalDown,
            Self::TopRight | Self::BottomLeft => LayoutCursor::ResizeDiagonalUp,
        }
    }
}

/// What the pointer is over in the preview: a layer's body (move) or one of
/// its resize handles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreviewTarget {
    pub layer: Layer,
    pub handle: Option<Handle>,
}

impl PreviewTarget {
    pub fn cursor(self) -> LayoutCursor {
        self.handle.map_or(LayoutCursor::Move, Handle::cursor)
    }
}

/// Mouse cursor over the preview.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutCursor {
    Move,
    ResizeHorizontal,
    ResizeVertical,
    /// Top-left / bottom-right corners.
    ResizeDiagonalDown,
    /// Top-right / bottom-left corners.
    ResizeDiagonalUp,
}

/// Everything the layout screen edits, as one undo step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutSnapshot {
    pub layout: ExportLayout,
    pub br_scale: f32,
    pub karaoke_text_scale: f32,
}

impl LayoutSnapshot {
    pub fn of(configuration: &ExportConfiguration) -> Self {
        Self {
            layout: configuration.layout,
            br_scale: configuration.br_scale,
            karaoke_text_scale: configuration.karaoke_text_scale,
        }
    }

    pub fn apply(self, configuration: &mut ExportConfiguration) {
        configuration.layout = self.layout;
        configuration.br_scale = self.br_scale;
        configuration.karaoke_text_scale = self.karaoke_text_scale;
    }
}

/// Undo / redo of the layout screen (Ctrl+Z, Ctrl+Y, Ctrl+Shift+Z).
#[derive(Clone, Debug, Default)]
pub struct LayoutHistory {
    undo: Vec<LayoutSnapshot>,
    redo: Vec<LayoutSnapshot>,
}

impl LayoutHistory {
    /// Records a change from `before` to `after`; nothing when equal.
    pub fn record(&mut self, before: LayoutSnapshot, after: LayoutSnapshot) {
        if before == after {
            return;
        }
        self.undo.push(before);
        if self.undo.len() > HISTORY_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// State to restore for an undo from `current`.
    pub fn undo(&mut self, current: LayoutSnapshot) -> Option<LayoutSnapshot> {
        let previous = self.undo.pop()?;
        self.redo.push(current);
        Some(previous)
    }

    /// State to restore for a redo from `current`.
    pub fn redo(&mut self, current: LayoutSnapshot) -> Option<LayoutSnapshot> {
        let next = self.redo.pop()?;
        self.undo.push(current);
        Some(next)
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

/// What the export window knows about the output, for the preview.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutPreviewContext {
    pub output_width: u32,
    pub output_height: u32,
    /// Band height at export, before the layout stretches it.
    pub band_height: u32,
    pub source_width: u32,
    pub source_height: u32,
    pub band_background: [f32; 4],
    pub playhead: [f32; 4],
}

impl LayoutPreviewContext {
    pub fn compose(&self, layout: &ExportLayout) -> ComposedLayout {
        export_layout::compose(
            self.output_width,
            self.output_height,
            self.band_height,
            self.source_width,
            self.source_height,
            layout,
        )
    }

    /// Size of a layer at 100 %, in output pixels.
    pub fn base_size(&self, layer: Layer) -> [f32; 2] {
        let (video, band) = export_layout::base_sizes(
            self.output_width,
            self.output_height,
            self.band_height,
            self.source_width,
            self.source_height,
        );
        match layer {
            Layer::Video => video,
            Layer::Band => band,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutDrag {
    pub layer: Layer,
    /// `None` moves the layer, a handle resizes it.
    pub handle: Option<Handle>,
    pub start_x: f32,
    pub start_y: f32,
    pub origin: LayerTransform,
}

/// Band texture the preview needs: the export renderer's output at a
/// reduced width. Its height follows from the width exactly like at export,
/// so the texture has the proportions of the exported band.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BandPreviewRequest {
    pub width: u32,
    /// Effective band scale passed to the renderer (as at export).
    pub br_scale: f32,
    /// Effective karaoke text scale passed to the renderer (as at export).
    pub karaoke_text_scale: f32,
}

/// A textured quad of the preview, clipped to the output frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TexturedDraw {
    pub rect: Rect,
    /// u_min, v_min, u_max, v_max.
    pub uv: [f32; 4],
}

/// What the UI draws over the preview with textures.
pub struct PreviewDraws {
    pub video: Option<TexturedDraw>,
    pub band: Option<TexturedDraw>,
    /// Layer outlines, drawn above the textures.
    pub outlines: Vec<QuadInstance>,
}

pub struct LayoutScreenView<'a> {
    pub context: LayoutPreviewContext,
    pub layout: &'a ExportLayout,
    pub values: &'a [String; VALUE_COUNT],
    pub resolution: &'a str,
    pub focused: usize,
}

pub enum LayoutPress {
    Adjust {
        control: usize,
        direction: i32,
    },
    /// Reset, back or launch button.
    Activate(usize),
    StartDrag(LayoutDrag),
}

fn layer_of(control: usize) -> Layer {
    if control < 4 {
        Layer::Video
    } else {
        Layer::Band
    }
}

/// Layer selected by the focused control (its steppers), if any.
pub fn selected_layer(focused: usize) -> Option<Layer> {
    (focused < STEPPER_COUNT).then(|| layer_of(focused))
}

/// First control of a layer's steppers.
pub fn first_control(layer: Layer) -> usize {
    match layer {
        Layer::Video => 0,
        Layer::Band => 4,
    }
}

fn transform_mut(layout: &mut ExportLayout, layer: Layer) -> &mut LayerTransform {
    match layer {
        Layer::Video => &mut layout.video,
        Layer::Band => &mut layout.band,
    }
}

pub fn control_name(control: usize) -> &'static str {
    match control {
        BR_SCALE_CONTROL => t("export_modal.br_scale"),
        KARAOKE_SCALE_CONTROL => t("export_modal.karaoke_text_scale"),
        RESET_CONTROL => t("export_layout.reset"),
        BACK_CONTROL => t("export_layout.back"),
        LAUNCH_CONTROL => t("export_layout.launch"),
        _ => match control % 4 {
            0 => t("export_layout.x"),
            1 => t("export_layout.y"),
            2 => t("export_layout.width"),
            _ => t("export_layout.height"),
        },
    }
}

pub fn layer_name(layer: Layer) -> &'static str {
    match layer {
        Layer::Video => t("export_layout.video"),
        Layer::Band => t("export_layout.band"),
    }
}

/// Accessible name of a control.
pub fn control_label(control: usize) -> String {
    if control < STEPPER_COUNT {
        format!(
            "{}, {}",
            layer_name(layer_of(control)),
            control_name(control)
        )
    } else {
        control_name(control).to_string()
    }
}

/// Values shown next to the controls, in the order of the controls.
pub fn display_values(configuration: &ExportConfiguration) -> [String; VALUE_COUNT] {
    let layer = |transform: &LayerTransform| {
        [
            format!("{:+.0} %", transform.offset_x * 100.0),
            format!("{:+.0} %", transform.offset_y * 100.0),
            format!("{:.0} %", transform.scale_x * 100.0),
            format!("{:.0} %", transform.scale_y * 100.0),
        ]
    };
    let [a, b, c, d] = layer(&configuration.layout.video);
    let [e, f, g, h] = layer(&configuration.layout.band);
    [
        a,
        b,
        c,
        d,
        e,
        f,
        g,
        h,
        format!("{:.0} %", configuration.br_scale * 100.0),
        format!("{:.0} %", configuration.karaoke_text_scale * 100.0),
    ]
}

/// Steps one value control. Returns whether the configuration changed.
pub fn adjust(configuration: &mut ExportConfiguration, control: usize, direction: i32) -> bool {
    if control >= VALUE_COUNT || direction == 0 {
        return false;
    }
    let direction = direction.signum() as f32;
    let step = |value: f32, step: f32| ((value + direction * step) / step).round() * step;
    match control {
        BR_SCALE_CONTROL => {
            let before = configuration.br_scale;
            configuration.br_scale = step(before, BR_SCALE_STEP).clamp(0.5, 2.0);
            (configuration.br_scale - before).abs() > f32::EPSILON
        }
        KARAOKE_SCALE_CONTROL => {
            let before = configuration.karaoke_text_scale;
            configuration.karaoke_text_scale = step(before, BR_SCALE_STEP).clamp(0.5, 2.0);
            (configuration.karaoke_text_scale - before).abs() > f32::EPSILON
        }
        _ => {
            let before = configuration.layout;
            let transform = transform_mut(&mut configuration.layout, layer_of(control));
            match control % 4 {
                0 => transform.offset_x = step(transform.offset_x, OFFSET_STEP),
                1 => transform.offset_y = step(transform.offset_y, OFFSET_STEP),
                2 => transform.scale_x = step(transform.scale_x, SCALE_STEP),
                _ => transform.scale_y = step(transform.scale_y, SCALE_STEP),
            }
            *transform = transform.normalized();
            configuration.layout != before
        }
    }
}

// ---------------------------------------------------------------------------
// Geometry
// ---------------------------------------------------------------------------

pub fn sidebar_rect(screen_w: f32, screen_h: f32) -> Rect {
    Rect {
        x: 0.0,
        y: 0.0,
        width: SIDEBAR_W.min(screen_w * 0.45).max(240.0),
        height: screen_h,
    }
}

/// Vertical distance between two control rows, fitted to the window.
fn row_step(sidebar: Rect) -> f32 {
    let bottom = sidebar.y + sidebar.height - BUTTON_H - 32.0;
    let available = bottom - (sidebar.y + FIRST_SECTION_Y) - 3.0 * HEADER_H - 3.0 * SECTION_GAP;
    (available / 11.0).clamp(28.0, 42.0)
}

/// Section of a value row: 0 video, 1 band, 2 sizes.
fn section_of(row: usize) -> usize {
    match row {
        0..=3 => 0,
        4..=7 => 1,
        _ => 2,
    }
}

fn section_first_row(section: usize) -> usize {
    [0, 4, 8][section.min(2)]
}

fn header_y(sidebar: Rect, section: usize) -> f32 {
    sidebar.y
        + FIRST_SECTION_Y
        + section as f32 * (HEADER_H + SECTION_GAP)
        + section_first_row(section) as f32 * row_step(sidebar)
}

/// Full row of a value control.
fn row_rect(sidebar: Rect, row: usize) -> Rect {
    let step = row_step(sidebar);
    let section = section_of(row);
    let y =
        header_y(sidebar, section) + HEADER_H + (row - section_first_row(section)) as f32 * step;
    Rect {
        x: sidebar.x + SIDEBAR_PAD,
        y,
        width: sidebar.width - 2.0 * SIDEBAR_PAD,
        height: step - 6.0,
    }
}

/// Rects of the label, "−", value and "+" parts of a value control.
pub fn stepper_rects(sidebar: Rect, control: usize) -> (Rect, Rect, Rect, Rect) {
    let row = row_rect(sidebar, control);
    let button_w = 32.0;
    let value_w = 72.0;
    let plus = Rect {
        x: row.x + row.width - button_w,
        y: row.y,
        width: button_w,
        height: row.height,
    };
    let value = Rect {
        x: plus.x - value_w - 2.0,
        y: row.y,
        width: value_w,
        height: row.height,
    };
    let minus = Rect {
        x: value.x - button_w - 2.0,
        y: row.y,
        width: button_w,
        height: row.height,
    };
    let label = Rect {
        x: row.x,
        y: row.y,
        width: (minus.x - row.x - 4.0).max(0.0),
        height: row.height,
    };
    (label, minus, value, plus)
}

pub fn reset_rect(sidebar: Rect) -> Rect {
    let last = row_rect(sidebar, VALUE_COUNT - 1);
    Rect {
        y: last.y + row_step(sidebar) + SECTION_GAP,
        ..last
    }
}

pub fn back_rect(sidebar: Rect) -> Rect {
    let width = (sidebar.width - 2.0 * SIDEBAR_PAD - 12.0) * 0.4;
    Rect {
        x: sidebar.x + SIDEBAR_PAD,
        y: sidebar.y + sidebar.height - BUTTON_H - 20.0,
        width,
        height: BUTTON_H,
    }
}

pub fn launch_rect(sidebar: Rect) -> Rect {
    let back = back_rect(sidebar);
    let x = back.x + back.width + 12.0;
    Rect {
        x,
        width: sidebar.x + sidebar.width - SIDEBAR_PAD - x,
        ..back
    }
}

pub fn control_rect(sidebar: Rect, control: usize) -> Rect {
    match control {
        RESET_CONTROL => reset_rect(sidebar),
        BACK_CONTROL => back_rect(sidebar),
        LAUNCH_CONTROL => launch_rect(sidebar),
        _ => {
            let row = row_rect(sidebar, control);
            Rect {
                x: row.x - 6.0,
                y: row.y - 3.0,
                width: row.width + 12.0,
                height: row.height + 6.0,
            }
        }
    }
}

/// Area on the right of the sidebar that holds the output frame.
pub fn preview_rect(screen_w: f32, screen_h: f32) -> Rect {
    let sidebar = sidebar_rect(screen_w, screen_h);
    let x = sidebar.x + sidebar.width + 24.0;
    Rect {
        x,
        y: 64.0,
        width: (screen_w - x - 24.0).max(40.0),
        height: (screen_h - 64.0 - 44.0).max(40.0),
    }
}

/// Output frame drawn inside the preview area, keeping its aspect ratio.
pub fn canvas_rect(preview: Rect, context: &LayoutPreviewContext) -> Rect {
    let width = context.output_width.max(1) as f32;
    let height = context.output_height.max(1) as f32;
    let scale = (preview.width / width).min(preview.height / height);
    Rect {
        x: preview.x + (preview.width - width * scale) / 2.0,
        y: preview.y + (preview.height - height * scale) / 2.0,
        width: width * scale,
        height: height * scale,
    }
}

/// On-screen rect of an output-pixel rect.
pub fn to_screen(canvas: Rect, context: &LayoutPreviewContext, rect: PixelRect) -> Rect {
    let scale = canvas.width / context.output_width.max(1) as f32;
    Rect {
        x: canvas.x + rect.x as f32 * scale,
        y: canvas.y + rect.y as f32 * scale,
        width: rect.width as f32 * scale,
        height: rect.height as f32 * scale,
    }
}

fn clip(rect: Rect, bounds: Rect) -> Option<Rect> {
    let x0 = rect.x.max(bounds.x);
    let y0 = rect.y.max(bounds.y);
    let x1 = (rect.x + rect.width).min(bounds.x + bounds.width);
    let y1 = (rect.y + rect.height).min(bounds.y + bounds.height);
    (x1 > x0 && y1 > y0).then_some(Rect {
        x: x0,
        y: y0,
        width: x1 - x0,
        height: y1 - y0,
    })
}

/// Clips a full-texture quad to `bounds`, cropping its texture coordinates
/// so the visible part keeps its place (like the export's overlay).
pub fn clip_textured(rect: Rect, bounds: Rect) -> Option<TexturedDraw> {
    let visible = clip(rect, bounds)?;
    let u = |x: f32| ((x - rect.x) / rect.width.max(f32::EPSILON)).clamp(0.0, 1.0);
    let v = |y: f32| ((y - rect.y) / rect.height.max(f32::EPSILON)).clamp(0.0, 1.0);
    Some(TexturedDraw {
        rect: visible,
        uv: [
            u(visible.x),
            v(visible.y),
            u(visible.x + visible.width),
            v(visible.y + visible.height),
        ],
    })
}

/// Width of the preview band texture for a band drawn `pixels` device
/// pixels wide (or as tall as a band that wide): rounded up so the texture
/// is only ever shrunk on screen, never stretched.
pub fn preview_band_width(pixels: f32) -> u32 {
    let pixels = if pixels.is_finite() {
        pixels.max(1.0)
    } else {
        1.0
    };
    let step = PREVIEW_BAND_WIDTH_STEP as f32;
    let width = ((pixels / step).ceil() * step).min(MAX_PREVIEW_BAND_WIDTH as f32);
    (width as u32).max(PREVIEW_BAND_WIDTH_STEP)
}

/// Band texture needed for the preview drawn on this screen. `pixel_scale`
/// is the number of device pixels per UI unit.
pub fn band_preview_request(
    screen_w: f32,
    screen_h: f32,
    pixel_scale: f32,
    context: &LayoutPreviewContext,
    configuration: &ExportConfiguration,
) -> BandPreviewRequest {
    let canvas = canvas_rect(preview_rect(screen_w, screen_h), context);
    // The band's height follows its width, so a band stretched in either
    // direction needs a texture wider than the canvas by that stretch to be
    // drawn at least 1:1 (like the supersampled band at export).
    let band = configuration.layout.normalized().band;
    let pixel_scale = if pixel_scale.is_finite() {
        pixel_scale.max(1.0)
    } else {
        1.0
    };
    let width = preview_band_width(canvas.width * band.scale_x.max(band.scale_y) * pixel_scale);
    let (br_scale, karaoke_text_scale) = crate::video_export::pipeline::effective_export_scales(
        configuration.br_scale,
        configuration.karaoke_text_scale,
    );
    BandPreviewRequest {
        width,
        br_scale,
        karaoke_text_scale,
    }
}

/// On-screen rect of a layer (it may extend past the output frame).
pub fn layer_screen_rect(
    screen_w: f32,
    screen_h: f32,
    context: &LayoutPreviewContext,
    layout: &ExportLayout,
    layer: Layer,
) -> Rect {
    let canvas = canvas_rect(preview_rect(screen_w, screen_h), context);
    let composed = context.compose(layout);
    let rect = match layer {
        Layer::Video => composed.video,
        Layer::Band => composed.band,
    };
    to_screen(canvas, context, rect)
}

/// Square drawn for a handle.
pub fn handle_rect(rect: Rect, handle: Handle) -> Rect {
    let (x, y) = handle.anchor(rect);
    Rect {
        x: x - HANDLE_SIZE / 2.0,
        y: y - HANDLE_SIZE / 2.0,
        width: HANDLE_SIZE,
        height: HANDLE_SIZE,
    }
}

/// Handle of a layer drawn at `rect` under the pointer: anywhere along an
/// edge (within a few pixels) or on a corner.
pub fn handle_at(rect: Rect, x: f32, y: f32) -> Option<Handle> {
    let (x0, y0) = (rect.x, rect.y);
    let (x1, y1) = (rect.x + rect.width, rect.y + rect.height);
    if x < x0 - HANDLE_GRAB || x > x1 + HANDLE_GRAB || y < y0 - HANDLE_GRAB || y > y1 + HANDLE_GRAB
    {
        return None;
    }
    // On a thin layer both edges are in reach: the closer one wins.
    let pick = |near_min: f32, near_max: f32| {
        let min = near_min <= HANDLE_GRAB;
        let max = near_max <= HANDLE_GRAB;
        if min && max {
            (near_min <= near_max, near_min > near_max)
        } else {
            (min, max)
        }
    };
    let (left, right) = pick((x - x0).abs(), (x - x1).abs());
    let (top, bottom) = pick((y - y0).abs(), (y - y1).abs());
    match (left, right, top, bottom) {
        (true, _, true, _) => Some(Handle::TopLeft),
        (_, true, true, _) => Some(Handle::TopRight),
        (true, _, _, true) => Some(Handle::BottomLeft),
        (_, true, _, true) => Some(Handle::BottomRight),
        (true, _, _, _) => Some(Handle::Left),
        (_, true, _, _) => Some(Handle::Right),
        (_, _, true, _) => Some(Handle::Top),
        (_, _, _, true) => Some(Handle::Bottom),
        _ => None,
    }
}

/// What the pointer at `(x, y)` would grab in the preview. Handles of the
/// selected layer come first, then the other layer's handles, then the
/// layers' bodies (the band is drawn above the video).
pub fn target_at(
    screen_w: f32,
    screen_h: f32,
    context: &LayoutPreviewContext,
    layout: &ExportLayout,
    selected: Option<Layer>,
    x: f32,
    y: f32,
) -> Option<PreviewTarget> {
    if sidebar_rect(screen_w, screen_h).contains(x, y) {
        return None;
    }
    let canvas = canvas_rect(preview_rect(screen_w, screen_h), context);
    let order = match selected {
        Some(Layer::Video) => [Layer::Video, Layer::Band],
        _ => [Layer::Band, Layer::Video],
    };
    for layer in order {
        let rect = layer_screen_rect(screen_w, screen_h, context, layout, layer);
        if let Some(handle) = handle_at(rect, x, y) {
            return Some(PreviewTarget {
                layer,
                handle: Some(handle),
            });
        }
    }
    for layer in [Layer::Band, Layer::Video] {
        let rect = layer_screen_rect(screen_w, screen_h, context, layout, layer);
        if clip(rect, canvas).is_some_and(|visible| visible.contains(x, y)) {
            return Some(PreviewTarget {
                layer,
                handle: None,
            });
        }
    }
    None
}

/// Where the video frame and the band texture go on screen, with the layer
/// outlines and the resize handles of the selected and hovered layers.
pub fn preview_draws(
    screen_w: f32,
    screen_h: f32,
    context: &LayoutPreviewContext,
    layout: &ExportLayout,
    drag: Option<&LayoutDrag>,
    focused: usize,
    hover: Option<PreviewTarget>,
) -> PreviewDraws {
    let canvas = canvas_rect(preview_rect(screen_w, screen_h), context);
    let video = layer_screen_rect(screen_w, screen_h, context, layout, Layer::Video);
    let band = layer_screen_rect(screen_w, screen_h, context, layout, Layer::Band);
    let selected = drag.map(|drag| drag.layer).or(selected_layer(focused));
    let mut outlines = Vec::new();
    for (layer, rect) in [(Layer::Video, video), (Layer::Band, band)] {
        let Some(visible) = clip(rect, canvas) else {
            continue;
        };
        let active = selected == Some(layer);
        let hovered = hover.is_some_and(|hover| hover.layer == layer);
        let color = if active {
            [0.38, 0.65, 1.0, 1.0]
        } else if hovered {
            [0.70, 0.82, 1.0, 0.75]
        } else {
            [1.0, 1.0, 1.0, 0.28]
        };
        outlines.push(outline_quad(visible, color, if active { 2.0 } else { 1.0 }));
    }
    // Handles above both outlines.
    for (layer, rect) in [(Layer::Video, video), (Layer::Band, band)] {
        let hovered = hover.is_some_and(|hover| hover.layer == layer);
        if selected != Some(layer) && !hovered {
            continue;
        }
        let hot = drag
            .filter(|drag| drag.layer == layer)
            .and_then(|drag| drag.handle)
            .or(hover
                .filter(|hover| hover.layer == layer)
                .and_then(|hover| hover.handle));
        for handle in Handle::ALL {
            outlines.push(handle_quad(handle_rect(rect, handle), hot == Some(handle)));
        }
    }
    PreviewDraws {
        video: clip_textured(video, canvas),
        band: clip_textured(band, canvas),
        outlines,
    }
}

/// What a click at `(x, y)` does on the screen.
pub fn press(
    screen_w: f32,
    screen_h: f32,
    context: &LayoutPreviewContext,
    layout: &ExportLayout,
    selected: Option<Layer>,
    x: f32,
    y: f32,
) -> Option<LayoutPress> {
    let sidebar = sidebar_rect(screen_w, screen_h);
    for control in 0..VALUE_COUNT {
        let (_, minus, _, plus) = stepper_rects(sidebar, control);
        if minus.contains(x, y) {
            return Some(LayoutPress::Adjust {
                control,
                direction: -1,
            });
        }
        if plus.contains(x, y) {
            return Some(LayoutPress::Adjust {
                control,
                direction: 1,
            });
        }
    }
    for control in [RESET_CONTROL, BACK_CONTROL, LAUNCH_CONTROL] {
        if control_rect(sidebar, control).contains(x, y) {
            return Some(LayoutPress::Activate(control));
        }
    }
    let target = target_at(screen_w, screen_h, context, layout, selected, x, y)?;
    let origin = match target.layer {
        Layer::Video => layout.video,
        Layer::Band => layout.band,
    };
    Some(LayoutPress::StartDrag(LayoutDrag {
        layer: target.layer,
        handle: target.handle,
        start_x: x,
        start_y: y,
        origin,
    }))
}

/// New scale and offset along one axis when an edge is dragged by `grow`
/// output pixels outwards. The opposite edge stays where it was: the layer
/// is stretched around its centre, so the centre moves by half the growth.
/// `side` is 1 for the right / bottom edge, -1 for the left / top edge.
pub fn resize_axis(
    origin_scale: f32,
    origin_offset: f32,
    base: f32,
    canvas: f32,
    grow: f32,
    side: f32,
) -> (f32, f32) {
    let base = base.max(1.0);
    let scale = ((origin_scale * base + grow) / base)
        .clamp(export_layout::MIN_SCALE, export_layout::MAX_SCALE);
    let scale = (scale * 1000.0).round() / 1000.0;
    let offset = origin_offset + side * (scale - origin_scale) * base / (2.0 * canvas.max(1.0));
    (scale, (offset * 10000.0).round() / 10000.0)
}

/// Moves the dragged layer to follow the pointer. Returns whether it moved.
pub fn drag_to(
    screen_w: f32,
    screen_h: f32,
    context: &LayoutPreviewContext,
    layout: &mut ExportLayout,
    drag: &LayoutDrag,
    x: f32,
    y: f32,
) -> bool {
    let canvas = canvas_rect(preview_rect(screen_w, screen_h), context);
    if canvas.width <= 0.0 || canvas.height <= 0.0 {
        return false;
    }
    let before = *layout;
    let base = context.base_size(drag.layer);
    let transform = transform_mut(layout, drag.layer);
    let Some(handle) = drag.handle else {
        let round = |value: f32| (value * 1000.0).round() / 1000.0;
        transform.offset_x = round(drag.origin.offset_x + (x - drag.start_x) / canvas.width);
        transform.offset_y = round(drag.origin.offset_y + (y - drag.start_y) / canvas.height);
        *transform = transform.normalized();
        return *layout != before;
    };
    // Pointer move in output pixels.
    let output_w = context.output_width.max(1) as f32;
    let output_h = context.output_height.max(1) as f32;
    let dx = (x - drag.start_x) * output_w / canvas.width;
    let dy = (y - drag.start_y) * output_h / canvas.height;
    let origin = drag.origin;
    let mut next = origin;
    if handle.left() || handle.right() {
        let (grow, side) = if handle.right() {
            (dx, 1.0)
        } else {
            (-dx, -1.0)
        };
        (next.scale_x, next.offset_x) =
            resize_axis(origin.scale_x, origin.offset_x, base[0], output_w, grow, side);
    }
    if handle.top() || handle.bottom() {
        let (grow, side) = if handle.bottom() {
            (dy, 1.0)
        } else {
            (-dy, -1.0)
        };
        (next.scale_y, next.offset_y) =
            resize_axis(origin.scale_y, origin.offset_y, base[1], output_h, grow, side);
    }
    *transform = next.normalized();
    *layout != before
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

pub fn render<'a>(
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    screen_w: f32,
    screen_h: f32,
    view: &LayoutScreenView<'a>,
) {
    let screen = Rect {
        x: 0.0,
        y: 0.0,
        width: screen_w,
        height: screen_h,
    };
    push_quad(quads, screen, [0.045, 0.048, 0.06, 1.0], [0.0; 4], 0.0);
    let sidebar = sidebar_rect(screen_w, screen_h);
    push_quad(
        quads,
        sidebar,
        [0.085, 0.09, 0.115, 1.0],
        [0.20, 0.23, 0.30, 1.0],
        0.0,
    );
    push_label(
        labels,
        t("export_layout.title"),
        Rect {
            x: sidebar.x + SIDEBAR_PAD,
            y: sidebar.y + 16.0,
            width: sidebar.width - 2.0 * SIDEBAR_PAD,
            height: 32.0,
        },
        HAlign::Left,
        21.0,
        Some([240, 242, 249]),
    );
    push_label(
        labels,
        t("export_layout.subtitle"),
        Rect {
            x: sidebar.x + SIDEBAR_PAD,
            y: sidebar.y + 48.0,
            width: sidebar.width - 2.0 * SIDEBAR_PAD,
            height: 20.0,
        },
        HAlign::Left,
        12.0,
        Some([141, 148, 168]),
    );

    for (section, title) in [
        t("export_layout.video"),
        t("export_layout.band"),
        t("export_layout.sizes"),
    ]
    .into_iter()
    .enumerate()
    {
        push_label(
            labels,
            title,
            Rect {
                x: sidebar.x + SIDEBAR_PAD,
                y: header_y(sidebar, section),
                width: sidebar.width - 2.0 * SIDEBAR_PAD,
                height: HEADER_H - 4.0,
            },
            HAlign::Left,
            13.0,
            Some([180, 186, 205]),
        );
    }
    for (control, value) in view.values.iter().enumerate() {
        let (label, minus, value_rect, plus) = stepper_rects(sidebar, control);
        push_label(
            labels,
            control_name(control),
            label,
            HAlign::Left,
            13.0,
            None,
        );
        for button in [minus, plus] {
            push_quad(
                quads,
                button,
                [0.12, 0.13, 0.17, 1.0],
                [0.22, 0.25, 0.32, 1.0],
                6.0,
            );
        }
        push_quad(
            quads,
            value_rect,
            [0.055, 0.06, 0.08, 1.0],
            [0.16, 0.18, 0.23, 1.0],
            4.0,
        );
        push_label(labels, "−", minus, HAlign::Center, 16.0, None);
        push_label(labels, value, value_rect, HAlign::Center, 12.0, None);
        push_label(labels, "+", plus, HAlign::Center, 16.0, None);
    }
    for (control, fill) in [
        (RESET_CONTROL, [0.12, 0.13, 0.17, 1.0]),
        (BACK_CONTROL, [0.12, 0.13, 0.17, 1.0]),
        (LAUNCH_CONTROL, [0.13, 0.42, 0.28, 1.0]),
    ] {
        let rect = control_rect(sidebar, control);
        push_quad(quads, rect, fill, [0.26, 0.30, 0.38, 1.0], 8.0);
        push_label(
            labels,
            control_name(control),
            rect,
            HAlign::Center,
            13.0,
            None,
        );
    }
    let focus = control_rect(sidebar, view.focused.min(CONTROL_COUNT - 1));
    quads.push(outline_quad(focus, [0.38, 0.65, 1.0, 1.0], 2.0));

    // Preview: black output frame; the video frame and the band texture are
    // drawn above it by the UI (see `preview_draws`).
    let preview = preview_rect(screen_w, screen_h);
    push_label(
        labels,
        t("export_layout.preview"),
        Rect {
            x: preview.x,
            y: 20.0,
            width: preview.width,
            height: 24.0,
        },
        HAlign::Left,
        14.0,
        Some([200, 205, 220]),
    );
    push_label(
        labels,
        t("export_layout.drag_hint"),
        Rect {
            x: preview.x,
            y: 20.0,
            width: preview.width,
            height: 24.0,
        },
        HAlign::Right,
        12.0,
        Some([132, 141, 160]),
    );
    let canvas = canvas_rect(preview, &view.context);
    push_quad(
        quads,
        canvas,
        [0.0, 0.0, 0.0, 1.0],
        [0.40, 0.42, 0.50, 1.0],
        0.0,
    );
    // Placeholders under the textures, visible until they are ready.
    let composed = view.context.compose(view.layout);
    if let Some(video) = clip(to_screen(canvas, &view.context, composed.video), canvas) {
        push_quad(quads, video, [0.08, 0.09, 0.12, 1.0], [0.0; 4], 0.0);
    }
    if let Some(band) = clip(to_screen(canvas, &view.context, composed.band), canvas) {
        push_quad(quads, band, view.context.band_background, [0.0; 4], 0.0);
    }
    push_label(
        labels,
        view.resolution,
        Rect {
            x: canvas.x,
            y: canvas.y + canvas.height + 6.0,
            width: canvas.width,
            height: 20.0,
        },
        HAlign::Right,
        11.0,
        Some([132, 141, 160]),
    );
}

fn outline_quad(rect: Rect, color: [f32; 4], width: f32) -> QuadInstance {
    QuadInstance {
        rect: [rect.x, rect.y, rect.width, rect.height],
        color: [0.0; 4],
        color_bottom: [0.0; 4],
        border_color: color,
        border_width: width,
        border_radius: 0.0,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    }
}

fn handle_quad(rect: Rect, hot: bool) -> QuadInstance {
    let fill = if hot {
        [0.38, 0.65, 1.0, 1.0]
    } else {
        [0.96, 0.97, 1.0, 1.0]
    };
    QuadInstance {
        rect: [rect.x, rect.y, rect.width, rect.height],
        color: fill,
        color_bottom: fill,
        border_color: [0.16, 0.36, 0.78, 1.0],
        border_width: 1.5,
        border_radius: 2.0,
        shadow_offset: [0.0, 1.0],
        shadow_color: [0.0, 0.0, 0.0, 0.45],
        shadow_blur: 3.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    }
}

fn push_label<'a>(
    labels: &mut Vec<LabelInfo<'a>>,
    text: &'a str,
    bounds: Rect,
    align: HAlign,
    size: f32,
    color: Option<[u8; 3]>,
) {
    labels.push(LabelInfo {
        text,
        bounds,
        h_align: align,
        v_align: VAlign::Center,
        overflow: Overflow::Ellipsis,
        padding: 6.0,
        font_size_override: Some(size),
        color_override: color,
        font_family_override: None,
    });
}

fn push_quad(
    quads: &mut Vec<QuadInstance>,
    rect: Rect,
    color: [f32; 4],
    border: [f32; 4],
    radius: f32,
) {
    quads.push(QuadInstance {
        rect: [rect.x, rect.y, rect.width, rect.height],
        color,
        color_bottom: color,
        border_color: border,
        border_width: if border[3] > 0.0 { 1.0 } else { 0.0 },
        border_radius: radius,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN_W: f32 = 1600.0;
    const SCREEN_H: f32 = 900.0;

    fn context() -> LayoutPreviewContext {
        LayoutPreviewContext {
            output_width: 1920,
            output_height: 1080,
            band_height: 180,
            source_width: 1920,
            source_height: 1080,
            band_background: [0.0; 4],
            playhead: [1.0; 4],
        }
    }

    #[test]
    fn steppers_move_by_whole_steps_and_clamp() {
        let mut configuration = ExportConfiguration::default();
        assert!(adjust(&mut configuration, 0, 1));
        assert!((configuration.layout.video.offset_x - 0.01).abs() < 1e-6);
        assert!(adjust(&mut configuration, 6, -1));
        assert!((configuration.layout.band.scale_x - 0.95).abs() < 1e-6);
        configuration.layout.band.scale_y = export_layout::MAX_SCALE;
        assert!(!adjust(&mut configuration, 7, 1));
        assert_eq!(display_values(&configuration)[6], "95 %");
        configuration.br_scale = 2.0;
        assert!(!adjust(&mut configuration, BR_SCALE_CONTROL, 1));
        assert!(adjust(&mut configuration, BR_SCALE_CONTROL, -1));
        assert!((configuration.br_scale - 1.9).abs() < 1e-5);
        assert!(adjust(&mut configuration, KARAOKE_SCALE_CONTROL, 1));
        assert!(!adjust(&mut configuration, RESET_CONTROL, 1));
    }

    #[test]
    fn dragging_the_band_in_the_preview_moves_it() {
        let context = context();
        let mut layout = ExportLayout::default();
        let canvas = canvas_rect(preview_rect(SCREEN_W, SCREEN_H), &context);
        // Bottom strip of the canvas is the band; its middle moves it (its
        // edges resize it).
        let band = layer_screen_rect(SCREEN_W, SCREEN_H, &context, &layout, Layer::Band);
        let x = canvas.x + canvas.width / 2.0;
        let y = band.y + band.height / 2.0;
        assert!(y > canvas.y + canvas.height * 0.8);
        let Some(LayoutPress::StartDrag(drag)) =
            press(SCREEN_W, SCREEN_H, &context, &layout, None, x, y)
        else {
            panic!("expected to grab the band");
        };
        assert_eq!(drag.layer, Layer::Band);
        assert_eq!(drag.handle, None);
        assert!(drag_to(
            SCREEN_W,
            SCREEN_H,
            &context,
            &mut layout,
            &drag,
            x - canvas.width * 0.25,
            y
        ));
        assert!((layout.band.offset_x + 0.25).abs() < 0.002);
        assert_eq!(layout.video, LayerTransform::default());
    }

    #[test]
    fn sidebar_buttons_are_pressable() {
        let sidebar = sidebar_rect(SCREEN_W, SCREEN_H);
        for control in [RESET_CONTROL, BACK_CONTROL, LAUNCH_CONTROL] {
            let rect = control_rect(sidebar, control);
            let press = press(
                SCREEN_W,
                SCREEN_H,
                &context(),
                &ExportLayout::default(),
                None,
                rect.x + rect.width / 2.0,
                rect.y + rect.height / 2.0,
            );
            assert!(matches!(press, Some(LayoutPress::Activate(c)) if c == control));
        }
    }

    #[test]
    fn controls_do_not_overlap_and_fit_the_sidebar() {
        for (w, h) in [(1600.0, 900.0), (1280.0, 720.0), (1024.0, 640.0)] {
            let sidebar = sidebar_rect(w, h);
            let rects: Vec<Rect> = (0..CONTROL_COUNT)
                .map(|control| control_rect(sidebar, control))
                .collect();
            for (i, a) in rects.iter().enumerate() {
                for b in rects.iter().skip(i + 1) {
                    let overlap = a.x < b.x + b.width
                        && b.x < a.x + a.width
                        && a.y < b.y + b.height
                        && b.y < a.y + a.height;
                    assert!(!overlap, "{a:?} overlaps {b:?} at {w}x{h}");
                }
                assert!(a.y + a.height <= sidebar.y + sidebar.height);
                assert!(a.x + a.width <= sidebar.x + sidebar.width);
            }
        }
    }

    #[test]
    fn preview_draws_match_the_composed_layout() {
        let context = context();
        let layout = ExportLayout {
            video: LayerTransform::default(),
            band: LayerTransform {
                offset_x: 0.0,
                offset_y: -0.3,
                scale_x: 0.5,
                scale_y: 1.5,
            },
        };
        let canvas = canvas_rect(preview_rect(SCREEN_W, SCREEN_H), &context);
        let draws = preview_draws(SCREEN_W, SCREEN_H, &context, &layout, None, 0, None);
        let composed = context.compose(&layout);
        let scale = canvas.width / 1920.0;
        let band = draws.band.expect("band visible");
        assert!((band.rect.width - composed.band.width as f32 * scale).abs() < 0.01);
        assert!((band.rect.height - composed.band.height as f32 * scale).abs() < 0.01);
        for (got, expected) in band.uv.iter().zip([0.0, 0.0, 1.0, 1.0]) {
            assert!((got - expected).abs() < 1e-4, "{:?}", band.uv);
        }
        let video = draws.video.expect("video visible");
        assert!((video.rect.width - composed.video.width as f32 * scale).abs() < 0.01);
        // Two outlines, then the eight handles of the selected video.
        assert_eq!(draws.outlines.len(), 2 + 8);
        let corner = handle_rect(
            layer_screen_rect(SCREEN_W, SCREEN_H, &context, &layout, Layer::Video),
            Handle::TopLeft,
        );
        assert_eq!(draws.outlines[2].rect[0], corner.x);
        // Hovering the band shows its handles too; nothing selected, none.
        let hover = Some(PreviewTarget {
            layer: Layer::Band,
            handle: Some(Handle::Right),
        });
        let draws = preview_draws(SCREEN_W, SCREEN_H, &context, &layout, None, 0, hover);
        assert_eq!(draws.outlines.len(), 2 + 16);
        let draws = preview_draws(
            SCREEN_W,
            SCREEN_H,
            &context,
            &layout,
            None,
            RESET_CONTROL,
            None,
        );
        assert_eq!(draws.outlines.len(), 2);
        // The preview keeps the output's aspect ratio.
        assert!((canvas.width / canvas.height - 1920.0 / 1080.0).abs() < 1e-3);
    }

    #[test]
    fn layers_past_the_frame_are_cropped_with_their_texture() {
        let bounds = Rect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        };
        let draw = clip_textured(
            Rect {
                x: 50.0,
                y: -50.0,
                width: 100.0,
                height: 100.0,
            },
            bounds,
        )
        .unwrap();
        assert_eq!(draw.rect.width, 50.0);
        assert_eq!(draw.rect.height, 50.0);
        assert_eq!(draw.uv, [0.0, 0.5, 0.5, 1.0]);
    }

    #[test]
    fn band_preview_request_uses_the_export_scales_and_output_proportions() {
        let context = context();
        let mut configuration = ExportConfiguration::default();
        configuration.br_scale = 1.0;
        configuration.karaoke_text_scale = 1.0;
        let request = band_preview_request(SCREEN_W, SCREEN_H, 1.0, &context, &configuration);
        assert_eq!(request.br_scale, 0.5);
        assert_eq!(request.karaoke_text_scale, 2.0);
        assert!(request.width <= MAX_PREVIEW_BAND_WIDTH);
        // At 100 %, the band is rendered as wide as it is drawn.
        let canvas = canvas_rect(preview_rect(SCREEN_W, SCREEN_H), &context);
        assert!(request.width as f32 >= canvas.width);
        assert!((request.width as f32) < canvas.width + PREVIEW_BAND_WIDTH_STEP as f32);
    }

    #[test]
    fn preview_band_follows_its_on_screen_size_so_text_stays_sharp() {
        let context = context();
        let canvas = canvas_rect(preview_rect(SCREEN_W, SCREEN_H), &context);
        let mut configuration = ExportConfiguration::default();
        let base = band_preview_request(SCREEN_W, SCREEN_H, 1.0, &context, &configuration).width;
        // 400 % wide: four times the texture, never stretched on screen.
        configuration.layout.band.scale_x = 4.0;
        let wide = band_preview_request(SCREEN_W, SCREEN_H, 1.0, &context, &configuration).width;
        assert!(wide as f32 >= canvas.width * 4.0);
        assert!(wide > base * 3);
        // Taller only: the height follows the width, so it is rendered wider too.
        configuration.layout.band.scale_x = 1.0;
        configuration.layout.band.scale_y = 2.0;
        let tall = band_preview_request(SCREEN_W, SCREEN_H, 1.0, &context, &configuration).width;
        assert!(tall as f32 >= canvas.width * 2.0);
        // High-DPI windows get device pixels.
        configuration.layout.band.scale_y = 1.0;
        let retina = band_preview_request(SCREEN_W, SCREEN_H, 2.0, &context, &configuration).width;
        assert!(retina as f32 >= canvas.width * 2.0);
        // Sizes are rounded up by steps (fewer re-renders) and capped.
        assert_eq!(preview_band_width(1.0), PREVIEW_BAND_WIDTH_STEP);
        assert_eq!(preview_band_width(65.0), 2 * PREVIEW_BAND_WIDTH_STEP);
        assert_eq!(preview_band_width(1e9), MAX_PREVIEW_BAND_WIDTH);
        assert_eq!(preview_band_width(f32::NAN), PREVIEW_BAND_WIDTH_STEP);
    }

    #[test]
    fn handles_are_found_on_edges_and_corners() {
        let rect = Rect {
            x: 100.0,
            y: 100.0,
            width: 200.0,
            height: 100.0,
        };
        assert_eq!(handle_at(rect, 100.0, 100.0), Some(Handle::TopLeft));
        assert_eq!(handle_at(rect, 304.0, 97.0), Some(Handle::TopRight));
        assert_eq!(handle_at(rect, 98.0, 202.0), Some(Handle::BottomLeft));
        assert_eq!(handle_at(rect, 300.0, 200.0), Some(Handle::BottomRight));
        assert_eq!(handle_at(rect, 103.0, 150.0), Some(Handle::Left));
        assert_eq!(handle_at(rect, 297.0, 130.0), Some(Handle::Right));
        assert_eq!(handle_at(rect, 160.0, 96.0), Some(Handle::Top));
        assert_eq!(handle_at(rect, 250.0, 205.0), Some(Handle::Bottom));
        // Inside, away from the edges: the body (move).
        assert_eq!(handle_at(rect, 200.0, 150.0), None);
        // Outside.
        assert_eq!(handle_at(rect, 80.0, 150.0), None);
        // A thin layer: the closer edge wins.
        let thin = Rect { height: 6.0, ..rect };
        assert_eq!(handle_at(thin, 200.0, 101.0), Some(Handle::Top));
        assert_eq!(handle_at(thin, 200.0, 105.0), Some(Handle::Bottom));
        // Every drawn handle is grabbed as itself.
        for handle in Handle::ALL {
            let square = handle_rect(rect, handle);
            let (x, y) = (square.x + square.width / 2.0, square.y + square.height / 2.0);
            assert_eq!(handle_at(rect, x, y), Some(handle));
        }
    }

    #[test]
    fn selected_layer_handles_win_and_bodies_move() {
        let context = context();
        let layout = ExportLayout::default();
        // The band's top edge is the video's bottom edge.
        let band = layer_screen_rect(SCREEN_W, SCREEN_H, &context, &layout, Layer::Band);
        let x = band.x + band.width / 2.0;
        let y = band.y;
        let target = |selected| target_at(SCREEN_W, SCREEN_H, &context, &layout, selected, x, y);
        assert_eq!(
            target(Some(Layer::Band)),
            Some(PreviewTarget {
                layer: Layer::Band,
                handle: Some(Handle::Top)
            })
        );
        assert_eq!(
            target(Some(Layer::Video)),
            Some(PreviewTarget {
                layer: Layer::Video,
                handle: Some(Handle::Bottom)
            })
        );
        let body = target_at(
            SCREEN_W,
            SCREEN_H,
            &context,
            &layout,
            None,
            x,
            band.y + band.height / 2.0,
        )
        .unwrap();
        assert_eq!(body.handle, None);
        assert_eq!(body.cursor(), LayoutCursor::Move);
        assert_eq!(Handle::Left.cursor(), LayoutCursor::ResizeHorizontal);
        assert_eq!(Handle::TopRight.cursor(), LayoutCursor::ResizeDiagonalUp);
        // The sidebar never grabs a layer.
        assert!(target_at(SCREEN_W, SCREEN_H, &context, &layout, None, 10.0, 10.0).is_none());
    }

    fn resize(layout: &mut ExportLayout, layer: Layer, handle: Handle, dx: f32, dy: f32) {
        let context = context();
        let rect = layer_screen_rect(SCREEN_W, SCREEN_H, &context, layout, layer);
        let square = handle_rect(rect, handle);
        let (x, y) = (square.x + square.width / 2.0, square.y + square.height / 2.0);
        let Some(LayoutPress::StartDrag(drag)) =
            press(SCREEN_W, SCREEN_H, &context, layout, Some(layer), x, y)
        else {
            panic!("expected to grab the {handle:?} handle");
        };
        assert_eq!(drag.handle, Some(handle));
        drag_to(SCREEN_W, SCREEN_H, &context, layout, &drag, x + dx, y + dy);
    }

    #[test]
    fn resizing_keeps_the_opposite_edge_in_place() {
        let context = context();
        let canvas = canvas_rect(preview_rect(SCREEN_W, SCREEN_H), &context);
        let px = canvas.width / 1920.0; // screen pixels per output pixel
        let mut layout = ExportLayout::default();
        let before = context.compose(&layout).band;

        // Right edge 480 output pixels to the right: 125 % wide, left edge fixed.
        resize(&mut layout, Layer::Band, Handle::Right, 480.0 * px, 0.0);
        let after = context.compose(&layout).band;
        assert!((layout.band.scale_x - 1.25).abs() < 0.002, "{layout:?}");
        assert!((after.x - before.x).abs() <= 2, "{before:?} -> {after:?}");
        assert_eq!(after.height, before.height);
        assert_eq!(after.y, before.y);

        // Left edge 240 px to the right: narrower, right edge fixed.
        let before = after;
        resize(&mut layout, Layer::Band, Handle::Left, 240.0 * px, 0.0);
        let after = context.compose(&layout).band;
        let right = |rect: PixelRect| rect.x + rect.width as i32;
        assert!((right(after) - right(before)).abs() <= 2, "{before:?} -> {after:?}");
        assert!((after.width as i32 - (before.width as i32 - 240)).abs() <= 2);

        // Top edge up by the band's height: twice as tall, bottom fixed.
        let before = after;
        resize(
            &mut layout,
            Layer::Band,
            Handle::Top,
            0.0,
            -(before.height as f32) * px,
        );
        let after = context.compose(&layout).band;
        let bottom = |rect: PixelRect| rect.y + rect.height as i32;
        assert!((layout.band.scale_y - 2.0).abs() < 0.01, "{layout:?}");
        assert!((bottom(after) - bottom(before)).abs() <= 2, "{before:?} -> {after:?}");
        assert_eq!(after.x, before.x);

        // A corner resizes both directions; the opposite corner stays.
        let mut layout = ExportLayout::default();
        let before = context.compose(&layout).video;
        resize(
            &mut layout,
            Layer::Video,
            Handle::BottomRight,
            -160.0 * px,
            -90.0 * px,
        );
        let after = context.compose(&layout).video;
        assert!((after.x - before.x).abs() <= 2 && (after.y - before.y).abs() <= 2);
        assert!((after.width as i32 - 1440).abs() <= 2, "{after:?}");
        assert!((after.height as i32 - 810).abs() <= 2, "{after:?}");
    }

    #[test]
    fn resizing_is_clamped_to_the_scale_limits() {
        let context = context();
        let canvas = canvas_rect(preview_rect(SCREEN_W, SCREEN_H), &context);
        let mut layout = ExportLayout::default();
        resize(&mut layout, Layer::Band, Handle::Right, canvas.width * 20.0, 0.0);
        assert_eq!(layout.band.scale_x, export_layout::MAX_SCALE);
        resize(&mut layout, Layer::Video, Handle::Top, 0.0, canvas.height * 5.0);
        assert_eq!(layout.video.scale_y, export_layout::MIN_SCALE);
        // The clamped axis keeps its opposite edge too.
        let (scale, offset) = resize_axis(1.0, 0.0, 1920.0, 1920.0, 1e6, 1.0);
        assert_eq!(scale, export_layout::MAX_SCALE);
        assert!((offset - 1.5).abs() < 1e-4);
    }

    #[test]
    fn history_undoes_and_redoes_whole_steps() {
        let mut configuration = ExportConfiguration::default();
        let mut history = LayoutHistory::default();
        assert!(!history.can_undo() && !history.can_redo());
        let start = LayoutSnapshot::of(&configuration);
        // A no-op is not a step.
        history.record(start, start);
        assert!(!history.can_undo());

        adjust(&mut configuration, 6, 1);
        let wide = LayoutSnapshot::of(&configuration);
        history.record(start, wide);
        adjust(&mut configuration, BR_SCALE_CONTROL, 1);
        let bigger = LayoutSnapshot::of(&configuration);
        history.record(wide, bigger);

        let restored = history.undo(bigger).unwrap();
        assert_eq!(restored, wide);
        restored.apply(&mut configuration);
        assert_eq!(configuration.br_scale, ExportConfiguration::default().br_scale);
        assert_eq!(history.undo(wide), Some(start));
        assert_eq!(history.undo(start), None);
        assert_eq!(history.redo(start), Some(wide));
        assert_eq!(history.redo(wide), Some(bigger));
        assert_eq!(history.redo(bigger), None);

        // A new change after an undo drops the redo branch.
        assert_eq!(history.undo(bigger), Some(wide));
        history.record(wide, start);
        assert!(!history.can_redo());

        // The history is bounded.
        let mut history = LayoutHistory::default();
        let mut other = start;
        for step in 0..(HISTORY_LIMIT + 10) {
            let before = other;
            other.layout.video.offset_x = step as f32 * 0.001 + 0.001;
            history.record(before, other);
        }
        let mut undone = 0;
        while let Some(previous) = history.undo(other) {
            other = previous;
            undone += 1;
        }
        assert_eq!(undone, HISTORY_LIMIT);
    }
}
