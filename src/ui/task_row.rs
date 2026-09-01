//! Non-blocking background task rows, in the spirit of Beautiful UI's
//! "Task Rows": a compact card at the top center of the screen lists the
//! running tasks (project load, export, proxy), each with an animated
//! spinner, a detail line and a progress bar. Clicking a row header unfolds
//! its sub-steps. Unlike the former modals, the card never dims the screen
//! nor captures input, so the UI stays interactive while workers run.

use super::primitives::{HAlign, LabelInfo, Overflow, QuadInstance, Rect, VAlign};

/// Identifies a task row slot so clicks can toggle its expanded state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskRowKind {
    Loading,
    Export,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStepState {
    Done,
    Running,
    Pending,
}

/// One sub-step shown when a row is expanded.
pub struct TaskStepView {
    pub label: String,
    pub state: TaskStepState,
    /// Right-aligned meta text (e.g. a percentage). Hidden when `None`.
    pub meta: Option<String>,
}

/// One visible background task.
pub struct TaskRowView {
    pub kind: TaskRowKind,
    /// Task name ("Export en cours…", "Chargement du projet…").
    pub title: String,
    /// Secondary line (file name). Hidden when `None`.
    pub detail: Option<String>,
    /// Progress fraction, 0.0..=1.0.
    pub progress: f32,
    /// Pre-formatted percentage label, kept here so rendered text can borrow
    /// from the row for the whole pass.
    pub percent: String,
    /// Show the "Escape to cancel" hint under the bar.
    pub cancellable: bool,
    /// Sub-steps revealed when expanded.
    pub steps: Vec<TaskStepView>,
    pub expanded: bool,
}

impl TaskRowView {
    pub fn new(
        kind: TaskRowKind,
        title: impl Into<String>,
        detail: Option<String>,
        progress: f32,
        cancellable: bool,
        steps: Vec<TaskStepView>,
        expanded: bool,
    ) -> Self {
        let progress = progress.clamp(0.0, 1.0);
        Self {
            kind,
            title: title.into(),
            detail,
            progress,
            percent: format!("{:.0} %", progress * 100.0),
            cancellable,
            steps,
            expanded,
        }
    }
}

/// Expanded state of every task row slot, owned by the UI shell.
#[derive(Default)]
pub struct TaskRowsState {
    pub loading_expanded: bool,
    pub export_expanded: bool,
}

/// Sub-step keys of a project load, in execution order.
pub const LOADING_STEP_KEYS: [&str; 5] = [
    "loading_project.reading_manifest",
    "loading_project.extracting_assets",
    "loading_project.verifying_assets",
    "loading_project.preparing_project",
    "loading_project.ready",
];

const CARD_W: f32 = 520.0;
const PAD_X: f32 = 18.0;
const PAD_TOP: f32 = 12.0;
const PAD_BOTTOM: f32 = 12.0;
const MARGIN_TOP: f32 = 14.0;
const SPINNER: f32 = 14.0;
const TITLE_H: f32 = 20.0;
const DETAIL_H: f32 = 14.0;
const BAR_H: f32 = 8.0;
const STEP_H: f32 = 18.0;
const HINT_H: f32 = 12.0;
const CHEVRON: f32 = 10.0;
const HEADER_H: f32 = PAD_TOP + TITLE_H + 6.0;
const SEPARATOR_H: f32 = 1.0;

fn shows_steps(row: &TaskRowView) -> bool {
    row.expanded && !row.steps.is_empty()
}

fn row_height(row: &TaskRowView) -> f32 {
    let mut h = PAD_TOP + TITLE_H;
    if row.detail.is_some() {
        h += 4.0 + DETAIL_H;
    }
    h += 8.0 + BAR_H;
    if shows_steps(row) {
        h += 8.0 + STEP_H * row.steps.len() as f32;
    }
    if row.cancellable {
        h += 6.0 + HINT_H;
    }
    h + PAD_BOTTOM
}

/// Card origin and per-row heights, shared by rendering and hit testing.
fn layout(rows: &[TaskRowView], screen_w: f32, screen_h: f32) -> Option<(f32, f32, f32)> {
    let _ = screen_h;
    if rows.is_empty() {
        return None;
    }
    let separators = rows.len().saturating_sub(1) as f32 * SEPARATOR_H;
    let card_h: f32 = rows.iter().map(row_height).sum::<f32>() + separators;
    let card_x = (screen_w - CARD_W) / 2.0;
    Some((card_x, MARGIN_TOP, card_h))
}

/// Bounds of the whole card, for click consumption.
pub fn card_bounds(
    rows: &[TaskRowView],
    screen_w: f32,
    screen_h: f32,
) -> Option<Rect> {
    layout(rows, screen_w, screen_h).map(|(card_x, card_y, card_h)| Rect {
        x: card_x,
        y: card_y,
        width: CARD_W,
        height: card_h,
    })
}

/// Header hit test: returns the row to toggle when (x, y) lands on its
/// header line.
pub fn row_header_at(
    rows: &[TaskRowView],
    x: f32,
    y: f32,
    screen_w: f32,
    screen_h: f32,
) -> Option<TaskRowKind> {
    let (card_x, card_y, _) = layout(rows, screen_w, screen_h)?;
    if !(card_x..card_x + CARD_W).contains(&x) {
        return None;
    }
    let mut row_y = card_y;
    for row in rows {
        if (row_y..row_y + HEADER_H).contains(&y) {
            return Some(row.kind);
        }
        row_y += row_height(row) + SEPARATOR_H;
    }
    None
}

/// Animated spinner: eight dots arranged in a circle, alpha chasing the
/// head. Driven by the 100 ms redraw tick of active jobs.
fn push_spinner(quads: &mut Vec<QuadInstance>, cx: f32, cy: f32) {
    const SEGMENTS: usize = 8;
    const ALPHAS: [f32; SEGMENTS] = [1.0, 0.85, 0.7, 0.55, 0.42, 0.3, 0.2, 0.14];
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f32())
        .unwrap_or(0.0);
    let head = (secs * 10.0) as usize % SEGMENTS;
    let radius = 5.5;
    let dot = 3.6;
    for i in 0..SEGMENTS {
        let age = (head + SEGMENTS - i) % SEGMENTS;
        let angle = (i as f32 / SEGMENTS as f32) * std::f32::consts::TAU
            - std::f32::consts::FRAC_PI_2;
        quads.push(QuadInstance {
            rect: [
                cx + radius * angle.cos() - dot / 2.0,
                cy + radius * angle.sin() - dot / 2.0,
                dot,
                dot,
            ],
            color: [0.55, 0.70, 1.0, ALPHAS[age]],
            color_bottom: [0.45, 0.60, 0.95, ALPHAS[age]],
            border_color: [0.0; 4],
            border_width: 0.0,
            border_radius: dot / 2.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
    }
}

fn push_bar(quads: &mut Vec<QuadInstance>, cx: f32, cy: f32, rotation: f32) {
    const LEN: f32 = 7.0;
    const THICK: f32 = 2.0;
    quads.push(QuadInstance {
        rect: [cx - LEN / 2.0, cy - THICK / 2.0, LEN, THICK],
        color: [0.62, 0.65, 0.75, 0.9],
        color_bottom: [0.62, 0.65, 0.75, 0.9],
        border_color: [0.0; 4],
        border_width: 0.0,
        border_radius: THICK / 2.0,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation,
        _padding: [0.0; 2],
    });
}

/// Small chevron built from two rotated bars: "v" expanded, ">" collapsed.
fn push_chevron(quads: &mut Vec<QuadInstance>, cx: f32, cy: f32, expanded: bool) {
    if expanded {
        push_bar(quads, cx - 2.2, cy - 0.7, 0.85);
        push_bar(quads, cx + 2.2, cy - 0.7, -0.85);
    } else {
        push_bar(quads, cx - 0.7, cy - 2.2, 0.72);
        push_bar(quads, cx - 0.7, cy + 2.2, 2.42);
    }
}

fn step_dot_color(state: TaskStepState) -> ([f32; 4], [f32; 4], f32) {
    match state {
        TaskStepState::Done => ([0.35, 0.78, 0.45, 1.0], [0.0; 4], 0.0),
        TaskStepState::Running => ([0.45, 0.65, 1.0, 1.0], [0.0; 4], 0.0),
        TaskStepState::Pending => (
            [0.0; 4],
            [0.45, 0.45, 0.52, 0.9],
            1.0,
        ),
    }
}

fn step_label_color(state: TaskStepState) -> [u8; 3] {
    match state {
        TaskStepState::Done => [140, 146, 160],
        TaskStepState::Running => [215, 220, 235],
        TaskStepState::Pending => [120, 124, 138],
    }
}

/// Render the stack at the top center. Nothing is emitted when `rows` is
/// empty.
pub fn render_task_rows<'a>(
    rows: &'a [TaskRowView],
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    screen_w: f32,
    screen_h: f32,
) {
    let Some((card_x, card_y, card_h)) = layout(rows, screen_w, screen_h) else {
        return;
    };

    // Card
    quads.push(QuadInstance {
        rect: [card_x, card_y, CARD_W, card_h],
        color: [0.15, 0.15, 0.19, 0.96],
        color_bottom: [0.11, 0.11, 0.14, 0.96],
        border_color: [0.40, 0.38, 0.55, 0.6],
        border_width: 1.0,
        border_radius: 12.0,
        shadow_offset: [0.0, 4.0],
        shadow_color: [0.0, 0.0, 0.0, 0.45],
        shadow_blur: 12.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    });

    let mut row_y = card_y;
    for (index, row) in rows.iter().enumerate() {
        if index > 0 {
            quads.push(QuadInstance {
                rect: [card_x + PAD_X, row_y, CARD_W - PAD_X * 2.0, SEPARATOR_H],
                color: [0.30, 0.30, 0.38, 0.6],
                color_bottom: [0.30, 0.30, 0.38, 0.6],
                border_color: [0.0; 4],
                border_width: 0.0,
                border_radius: 0.0,
                shadow_offset: [0.0; 2],
                shadow_color: [0.0; 4],
                shadow_blur: 0.0,
                rotation: 0.0,
                _padding: [0.0; 2],
            });
            row_y += SEPARATOR_H;
        }

        let content_x = card_x + PAD_X;
        let title_y = row_y + PAD_TOP;
        let title_mid = title_y + TITLE_H / 2.0;

        // Spinner
        push_spinner(quads, content_x + SPINNER / 2.0, title_mid);

        // Chevron (rows always have at least one step)
        let chevron_cx = card_x + CARD_W - PAD_X - CHEVRON / 2.0;
        push_chevron(quads, chevron_cx, title_mid, row.expanded);

        let text_x = content_x + SPINNER + 10.0;
        let right_w = 56.0;
        let text_w = chevron_cx - CHEVRON / 2.0 - 10.0 - text_x;

        // Title
        labels.push(LabelInfo {
            text: row.title.as_str(),
            bounds: Rect {
                x: text_x,
                y: title_y,
                width: text_w - right_w,
                height: TITLE_H,
            },
            h_align: HAlign::Left,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(15.0),
            color_override: Some([232, 234, 245]),
            font_family_override: None,
        });

        // Percentage, right-aligned before the chevron
        labels.push(LabelInfo {
            text: row.percent.as_str(),
            bounds: Rect {
                x: text_x + text_w - right_w,
                y: title_y,
                width: right_w,
                height: TITLE_H,
            },
            h_align: HAlign::Right,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(13.0),
            color_override: Some([170, 175, 195]),
            font_family_override: None,
        });

        let mut cursor_y = title_y + TITLE_H;

        // Detail line
        if let Some(detail) = row.detail.as_deref() {
            cursor_y += 4.0;
            labels.push(LabelInfo {
                text: detail,
                bounds: Rect {
                    x: text_x,
                    y: cursor_y,
                    width: text_w,
                    height: DETAIL_H,
                },
                h_align: HAlign::Left,
                v_align: VAlign::Center,
                overflow: Overflow::Clip,
                padding: 0.0,
                font_size_override: Some(12.0),
                color_override: Some([160, 165, 185]),
                font_family_override: None,
            });
            cursor_y += DETAIL_H;
        }

        // Progress bar
        cursor_y += 8.0;
        let bar_x = content_x;
        let bar_w = CARD_W - PAD_X * 2.0;
        quads.push(QuadInstance {
            rect: [bar_x, cursor_y, bar_w, BAR_H],
            color: [0.10, 0.10, 0.13, 1.0],
            color_bottom: [0.10, 0.10, 0.13, 1.0],
            border_color: [0.30, 0.30, 0.38, 0.8],
            border_width: 1.0,
            border_radius: 4.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
        let fill = (bar_w - 4.0) * row.progress;
        if fill > 0.5 {
            quads.push(QuadInstance {
                rect: [bar_x + 2.0, cursor_y + 2.0, fill, BAR_H - 4.0],
                color: [0.35, 0.60, 1.0, 1.0],
                color_bottom: [0.25, 0.45, 0.85, 1.0],
                border_color: [0.0; 4],
                border_width: 0.0,
                border_radius: 3.0,
                shadow_offset: [0.0; 2],
                shadow_color: [0.0; 4],
                shadow_blur: 0.0,
                rotation: 0.0,
                _padding: [0.0; 2],
            });
        }
        cursor_y += BAR_H;

        // Sub-steps, revealed when the row is expanded
        if shows_steps(row) {
            cursor_y += 8.0;
            let step_dot = 7.0;
            for step in &row.steps {
                let (fill_color, border_color, border_width) = step_dot_color(step.state);
                quads.push(QuadInstance {
                    rect: [
                        text_x,
                        cursor_y + (STEP_H - step_dot) / 2.0,
                        step_dot,
                        step_dot,
                    ],
                    color: fill_color,
                    color_bottom: fill_color,
                    border_color,
                    border_width,
                    border_radius: step_dot / 2.0,
                    shadow_offset: [0.0; 2],
                    shadow_color: [0.0; 4],
                    shadow_blur: 0.0,
                    rotation: 0.0,
                    _padding: [0.0; 2],
                });
                labels.push(LabelInfo {
                    text: step.label.as_str(),
                    bounds: Rect {
                        x: text_x + step_dot + 8.0,
                        y: cursor_y,
                        width: text_w - step_dot - 8.0 - 60.0,
                        height: STEP_H,
                    },
                    h_align: HAlign::Left,
                    v_align: VAlign::Center,
                    overflow: Overflow::Clip,
                    padding: 0.0,
                    font_size_override: Some(12.0),
                    color_override: Some(step_label_color(step.state)),
                    font_family_override: None,
                });
                if let Some(meta) = step.meta.as_deref() {
                    labels.push(LabelInfo {
                        text: meta,
                        bounds: Rect {
                            x: text_x + text_w - 60.0,
                            y: cursor_y,
                            width: 60.0,
                            height: STEP_H,
                        },
                        h_align: HAlign::Right,
                        v_align: VAlign::Center,
                        overflow: Overflow::Clip,
                        padding: 0.0,
                        font_size_override: Some(11.0),
                        color_override: Some([150, 156, 175]),
                        font_family_override: None,
                    });
                }
                cursor_y += STEP_H;
            }
        }

        // Cancellation hint
        if row.cancellable {
            cursor_y += 6.0;
            labels.push(LabelInfo {
                text: crate::i18n::t("progress.cancel_hint"),
                bounds: Rect {
                    x: text_x,
                    y: cursor_y,
                    width: text_w,
                    height: HINT_H,
                },
                h_align: HAlign::Left,
                v_align: VAlign::Center,
                overflow: Overflow::Clip,
                padding: 0.0,
                font_size_override: Some(11.0),
                color_override: Some([130, 134, 150]),
                font_family_override: None,
            });
        }

        row_y += row_height(row);
    }
}
