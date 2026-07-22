use crate::constants;
use crate::project::Project;
use crate::render_index::ProjectRenderIndex;
use crate::rendering::rythmo::scene::{
    karaoke_adjacent_max_gap_frames, karaoke_count_in_frames, karaoke_stack_height,
    karaoke_stack_y, FrameWindow, RythmoScene, SceneOptions,
};
use crate::rythmo_layout;

pub(crate) const BACKGROUND: [u8; 4] = [5, 5, 8, 255];
pub(crate) const GPU_PLAYHEAD: [u8; 4] = [217, 38, 38, 255];
const BASE_PLAYHEAD_WIDTH: f32 = 3.0;

pub(crate) struct ExportPlan {
    pub width: u32,
    pub height: u32,
    pub virtual_width: u32,
    pub virtual_scale: f32,
    pub crop_left: i64,
    pub overlays: Vec<KaraokeOverlay>,
    pub centered_playhead: PixelRect,
    pub shifted_playhead: PixelRect,
}

impl ExportPlan {
    pub fn new(
        project: &Project,
        current_frame: f64,
        width: u32,
        source_fps: f64,
        br_scale: f32,
        karaoke_text_scale: f32,
    ) -> Self {
        let scale = width as f32 / constants::REF_WIDTH * br_scale;
        let ruler_height = constants::RULER_HEIGHT * scale;
        let badge_gap = constants::BADGE_GAP * scale;
        let slot_header_height = (constants::BADGE_HEIGHT * scale)
            .max(constants::VOICE_ACTOR_DISPLAY_ICON_SIZE * scale);
        let font_size = constants::RYTHMO_FONT_SIZE * scale;
        let playhead_width = BASE_PLAYHEAD_WIDTH * scale;
        let center_x = width as f32 * 0.5;
        let ppf = constants::PIXELS_PER_FRAME * scale * crate::config::scroll_speed();
        let visible_frames = (width as f32 / ppf.max(0.001)) as i64 + 4;
        let mut render_index = ProjectRenderIndex::new();
        render_index.refresh(project);
        let margin = ((source_fps.max(1.0) * 10.0).round() as i64)
            .max(karaoke_adjacent_max_gap_frames(source_fps))
            .max(karaoke_count_in_frames(source_fps))
            .saturating_add(render_index.max_duration_frames());
        let scene = RythmoScene::build(
            project,
            &render_index,
            SceneOptions {
                frame_window: FrameWindow {
                    first: (current_frame.floor() as i64)
                        .saturating_sub(visible_frames / 2)
                        .saturating_sub(margin),
                    last: (current_frame.ceil() as i64)
                        .saturating_add(visible_frames / 2)
                        .saturating_add(margin),
                },
                current_frame,
                source_fps,
                normal_body_height: constants::SLOT_HEIGHT * scale,
                slot_header_height,
                badge_gap,
                scale,
                dynamic_track_layout: false,
            },
        );
        let height =
            (ruler_height + rythmo_layout::total_tracks_height(&scene.tracks)).ceil() as u32;
        let desired_left = crate::config::playhead_x(0.0, width as f32, playhead_width);
        let extra = (center_x - desired_left).abs().ceil() as u32 * 2 + 16;
        let virtual_width = width.saturating_add(extra).max(width);
        let virtual_scale = br_scale * width as f32 / virtual_width.max(1) as f32;
        let virtual_left = virtual_width as f32 * 0.5 - playhead_width * 0.5;
        let crop_left = (virtual_left - desired_left).round() as i64;
        let centered_playhead = PixelRect::from_f32(
            center_x - playhead_width * 0.5,
            0.0,
            playhead_width.max(1.0),
            height as f32,
        );
        let shifted_playhead = PixelRect::from_f32(
            desired_left,
            0.0,
            playhead_width.max(1.0),
            height as f32,
        );
        let overlays = build_overlays(
            &scene,
            center_x,
            ruler_height,
            slot_header_height,
            badge_gap,
            scale,
            font_size,
            karaoke_text_scale,
        );
        Self {
            width,
            height,
            virtual_width,
            virtual_scale,
            crop_left,
            overlays,
            centered_playhead,
            shifted_playhead,
        }
    }
}

fn build_overlays(
    scene: &RythmoScene,
    center_x: f32,
    ruler_height: f32,
    slot_header_height: f32,
    badge_gap: f32,
    scale: f32,
    font_size: f32,
    karaoke_text_scale: f32,
) -> Vec<KaraokeOverlay> {
    scene
        .lines
        .iter()
        .filter(|line| line.karaoke_should_be_centered())
        .filter_map(|scene_line| {
            let track = rythmo_layout::track_for_y_slot(&scene.tracks, scene_line.line.y_slot)?;
            let body_y = ruler_height + track.top + slot_header_height + badge_gap;
            let line_y = karaoke_stack_y(
                body_y,
                track.body_h,
                scene_line.karaoke_stack_row,
                scale,
            );
            let body_h = karaoke_stack_height(track.body_h, scale);
            let karaoke_font =
                font_size * constants::KARAOKE_TEXT_FONT_SCALE * karaoke_text_scale;
            let text_w = crate::vector_text::measure_rythmo_text_width_standalone(
                &scene_line.line.text,
                karaoke_font,
            )
            .map(|w| w.ceil() + 1.0)
            .unwrap_or_else(|| {
                scene_line.line.text.chars().count().max(1) as f32 * karaoke_font * 0.62
                    + karaoke_font * 0.7
            })
            .max(2.0);
            let text_left = center_x - text_w * 0.5;
            let badge_w = scene_line
                .character_label_visible
                .then(|| {
                    rythmo_layout::scaled_character_badge_width(
                        &scene_line.line.character_name,
                        scale,
                    )
                })
                .unwrap_or(0.0);
            let dot_size = constants::KARAOKE_DOT_SIZE * scale.max(0.5);
            let travel = constants::KARAOKE_NEXT_PREVIEW_GAP * 4.0 * scale + dot_size * 2.0;
            Some(KaraokeOverlay {
                copy_rect: PixelRect::from_edges(
                    (text_left - travel).min(text_left - badge_w - badge_gap) - 4.0,
                    line_y - dot_size * constants::KARAOKE_DOT_BOUNCE_AMPLITUDE - 5.0,
                    text_left + text_w + 4.0,
                    line_y + body_h + 5.0,
                ),
                text_rect: PixelRect::from_f32(text_left, line_y, text_w, body_h),
            })
        })
        .collect()
}

#[derive(Clone, Copy)]
pub(crate) struct KaraokeOverlay {
    pub copy_rect: PixelRect,
    pub text_rect: PixelRect,
}

#[derive(Clone, Copy, Default)]
pub(crate) struct PixelRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl PixelRect {
    fn from_f32(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self::from_edges(x, y, x + width, y + height)
    }

    fn from_edges(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self {
            left: left.floor() as i32,
            top: top.floor() as i32,
            right: right.ceil() as i32,
            bottom: bottom.ceil() as i32,
        }
    }

    pub fn clipped(self, width: u32, height: u32) -> Self {
        Self {
            left: self.left.clamp(0, width as i32),
            top: self.top.clamp(0, height as i32),
            right: self.right.clamp(0, width as i32),
            bottom: self.bottom.clamp(0, height as i32),
        }
    }
}

pub(crate) fn intersects(a: PixelRect, b: PixelRect) -> bool {
    a.left < b.right && a.right > b.left && a.top < b.bottom && a.bottom > b.top
}

pub(crate) fn crop_rgba(
    source: &[u8],
    source_width: u32,
    output_width: u32,
    height: u32,
    crop_left: i64,
) -> Vec<u8> {
    let mut output = vec![0; output_width as usize * height as usize * 4];
    output
        .chunks_exact_mut(4)
        .for_each(|pixel| pixel.copy_from_slice(&BACKGROUND));
    let source_height = source
        .len()
        .checked_div(source_width.max(1) as usize * 4)
        .unwrap_or(0)
        .min(height as usize);
    for y in 0..source_height {
        for x in 0..output_width as i64 {
            let sx = x + crop_left;
            if !(0..source_width as i64).contains(&sx) {
                continue;
            }
            let src = (y * source_width as usize + sx as usize) * 4;
            let dst = (y * output_width as usize + x as usize) * 4;
            output[dst..dst + 4].copy_from_slice(&source[src..src + 4]);
        }
    }
    output
}

pub(crate) fn copy_rgba_rect(
    source: &[u8],
    destination: &mut [u8],
    width: u32,
    height: u32,
    rect: PixelRect,
) {
    let rect = rect.clipped(width, height);
    for y in rect.top..rect.bottom {
        let start = ((y as u32 * width + rect.left as u32) * 4) as usize;
        let end = ((y as u32 * width + rect.right as u32) * 4) as usize;
        destination[start..end].copy_from_slice(&source[start..end]);
    }
}

pub(crate) fn copy_rgba_intersection(
    source: &[u8],
    destination: &mut [u8],
    width: u32,
    height: u32,
    a: PixelRect,
    b: PixelRect,
) {
    let rect = PixelRect {
        left: a.left.max(b.left),
        top: a.top.max(b.top),
        right: a.right.min(b.right),
        bottom: a.bottom.min(b.bottom),
    };
    if rect.right > rect.left && rect.bottom > rect.top {
        copy_rgba_rect(source, destination, width, height, rect);
    }
}

pub(crate) fn restore_playhead_rgba(
    source: &[u8],
    destination: &mut [u8],
    width: u32,
    height: u32,
    area: PixelRect,
    column: PixelRect,
) {
    let rect = PixelRect {
        left: area.left.max(column.left),
        top: area.top.max(column.top),
        right: area.right.min(column.right),
        bottom: area.bottom.min(column.bottom),
    }
    .clipped(width, height);
    for y in rect.top..rect.bottom {
        for x in rect.left..rect.right {
            let index = ((y as u32 * width + x as u32) * 4) as usize;
            if destination[index..index + 4] == GPU_PLAYHEAD {
                destination[index..index + 4].copy_from_slice(&source[index..index + 4]);
            }
        }
    }
}
