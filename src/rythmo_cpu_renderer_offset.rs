//! Offset-aware facade for the CPU export renderer.
//!
//! The legacy renderer is kept intact. For a shifted playhead we render the
//! scrolling timeline on a wider surface, crop it around the configured
//! playhead, then composite the fixed karaoke overlays back at the centre.

#[path = "rythmo_cpu_renderer.rs"]
mod implementation;

use std::collections::HashSet;

use crate::constants;
use crate::project::Project;
use crate::render_index::ProjectRenderIndex;
use crate::rendering::rythmo::scene::{
    karaoke_adjacent_max_gap_frames, karaoke_count_in_frames, karaoke_stack_height,
    karaoke_stack_y, FrameWindow, RythmoScene, SceneOptions,
};
use crate::rythmo_layout;

const BACKGROUND: [u8; 4] = [5, 5, 8, 255];
const CPU_PLAYHEAD: [u8; 4] = [255, 5, 13, 255];
const BASE_PLAYHEAD_WIDTH: f32 = 3.0;

pub struct CpuRenderer {
    full: implementation::CpuRenderer,
    timeline: implementation::CpuRenderer,
}

impl Default for CpuRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl CpuRenderer {
    pub fn new() -> Self {
        Self {
            full: implementation::CpuRenderer::new(),
            timeline: implementation::CpuRenderer::new(),
        }
    }

    pub fn render_br(
        &mut self,
        project: &Project,
        current_frame: f64,
        width: u32,
        source_fps: f64,
        br_scale: f32,
        karaoke_text_scale: f32,
    ) -> Vec<u8> {
        let offset = crate::config::playhead_offset_percent();
        if offset.abs() <= f32::EPSILON || width == 0 {
            return self.full.render_br(
                project,
                current_frame,
                width,
                source_fps,
                br_scale,
                karaoke_text_scale,
            );
        }

        let full = self.full.render_br(
            project,
            current_frame,
            width,
            source_fps,
            br_scale,
            karaoke_text_scale,
        );
        let Some(height) = rgba_height(&full, width) else {
            return full;
        };

        let geometry = ExportGeometry::new(
            project,
            current_frame,
            width,
            source_fps,
            br_scale,
            karaoke_text_scale,
        );
        let centered_ids: HashSet<u64> = geometry
            .scene
            .lines
            .iter()
            .filter(|line| line.karaoke_should_be_centered())
            .map(|line| line.line.id)
            .collect();

        let mut timeline_project = project.snapshot();
        if !centered_ids.is_empty() {
            timeline_project.retain_lines(|line| !centered_ids.contains(&line.id));
        }

        // Keep the original export scale while widening the render target. This
        // gives the crop enough future content on the right, instead of leaving
        // a blank strip or making lines pop at the viewport edge.
        let desired_playhead_left = crate::config::playhead_x(
            0.0,
            width as f32,
            geometry.playhead_width,
        );
        let required_extra = (geometry.center_x - desired_playhead_left)
            .abs()
            .ceil() as u32;
        let extra = required_extra.saturating_mul(2).saturating_add(16);
        let virtual_width = width.saturating_add(extra).max(width);
        let virtual_scale = if virtual_width == 0 {
            br_scale
        } else {
            br_scale * width as f32 / virtual_width as f32
        };
        let virtual_playhead_left =
            virtual_width as f32 * 0.5 - geometry.playhead_width * 0.5;
        let crop_left = (virtual_playhead_left - desired_playhead_left).round() as i64;

        let wide = self.timeline.render_br(
            &timeline_project,
            current_frame,
            virtual_width,
            source_fps,
            virtual_scale,
            karaoke_text_scale,
        );
        let mut output = crop_rgba(&wide, virtual_width, width, height, crop_left);
        if output.len() != full.len() {
            return full;
        }
        let timeline = output.clone();

        for overlay in geometry.overlays() {
            copy_rect(&full, &mut output, width, height, overlay.copy_rect);

            // The fixed overlay source was rendered with the old centred
            // playhead. Remove only its exact solid-red pixels, never glyphs.
            restore_matching_pixels(
                &timeline,
                &mut output,
                width,
                height,
                overlay.copy_rect,
                geometry.centered_playhead_rect(),
                CPU_PLAYHEAD,
            );

            // Compositing a fixed karaoke rectangle may cover the shifted
            // playhead. Put it back unless it actually crosses the karaoke text.
            if !rects_intersect(overlay.text_rect, geometry.shifted_playhead_rect(height)) {
                copy_intersection(
                    &timeline,
                    &mut output,
                    width,
                    height,
                    overlay.copy_rect,
                    geometry.shifted_playhead_rect(height),
                );
            }
        }

        output
    }
}

struct ExportGeometry {
    scene: RythmoScene,
    width: u32,
    height: u32,
    scale: f32,
    ruler_height: f32,
    slot_header_height: f32,
    badge_gap: f32,
    font_size: f32,
    karaoke_text_scale: f32,
    center_x: f32,
    playhead_width: f32,
}

impl ExportGeometry {
    fn new(
        project: &Project,
        current_frame: f64,
        width: u32,
        source_fps: f64,
        br_scale: f32,
        karaoke_text_scale: f32,
    ) -> Self {
        let scale = width as f32 / constants::REF_WIDTH * br_scale;
        let normal_slot_height = constants::SLOT_HEIGHT * scale;
        let ruler_height = constants::RULER_HEIGHT * scale;
        let badge_height = constants::BADGE_HEIGHT * scale;
        let badge_gap = constants::BADGE_GAP * scale;
        let actor_icon_size = constants::VOICE_ACTOR_DISPLAY_ICON_SIZE * scale;
        let slot_header_height = badge_height.max(actor_icon_size);
        let font_size = constants::RYTHMO_FONT_SIZE * scale;
        let ppf = constants::PIXELS_PER_FRAME * scale * crate::config::scroll_speed();
        let visible_frames = (width as f32 / ppf.max(0.001)) as i64 + 4;
        let mut render_index = ProjectRenderIndex::new();
        render_index.refresh(project);
        let render_margin = ((source_fps.max(1.0) * 10.0).round() as i64)
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
                        .saturating_sub(render_margin),
                    last: (current_frame.ceil() as i64)
                        .saturating_add(visible_frames / 2)
                        .saturating_add(render_margin),
                },
                current_frame,
                source_fps,
                normal_body_height: normal_slot_height,
                slot_header_height,
                badge_gap,
                scale,
                dynamic_track_layout: false,
            },
        );
        let height = (ruler_height + rythmo_layout::total_tracks_height(&scene.tracks)).ceil() as u32;
        Self {
            scene,
            width,
            height,
            scale,
            ruler_height,
            slot_header_height,
            badge_gap,
            font_size,
            karaoke_text_scale,
            center_x: width as f32 * 0.5,
            playhead_width: BASE_PLAYHEAD_WIDTH * scale,
        }
    }

    fn overlays(&self) -> Vec<KaraokeOverlay> {
        let mut overlays = Vec::new();
        for scene_line in self
            .scene
            .lines
            .iter()
            .filter(|line| line.karaoke_should_be_centered())
        {
            let Some(track) = rythmo_layout::track_for_y_slot(&self.scene.tracks, scene_line.line.y_slot)
            else {
                continue;
            };
            let body_y = self.ruler_height + track.top + self.slot_header_height + self.badge_gap;
            let line_y = karaoke_stack_y(
                body_y,
                track.body_h,
                scene_line.karaoke_stack_row,
                self.scale,
            );
            let body_h = karaoke_stack_height(track.body_h, self.scale);
            let karaoke_font = self.font_size
                * constants::KARAOKE_TEXT_FONT_SCALE
                * self.karaoke_text_scale;
            let text_w = crate::vector_text::measure_rythmo_text_width_standalone(
                &scene_line.line.text,
                karaoke_font,
            )
            .map(|width| width.ceil() + 1.0)
            .unwrap_or_else(|| {
                let count = scene_line.line.text.chars().count().max(1) as f32;
                count * karaoke_font * 0.62 + karaoke_font * 0.7
            })
            .max(2.0);
            let text_left = self.center_x - text_w * 0.5;
            let text_rect = PixelRect::from_f32(text_left, line_y, text_w, body_h);

            let badge_w = if scene_line.character_label_visible {
                rythmo_layout::scaled_character_badge_width(
                    &scene_line.line.character_name,
                    self.scale,
                )
            } else {
                0.0
            };
            let dot_size = constants::KARAOKE_DOT_SIZE * self.scale.max(0.5);
            let count_in_travel = constants::KARAOKE_NEXT_PREVIEW_GAP * 4.0 * self.scale
                + dot_size * 2.0;
            let left = (text_left - count_in_travel)
                .min(text_left - badge_w - self.badge_gap)
                - 4.0;
            let right = text_left + text_w + 4.0;
            let top = line_y
                - dot_size * constants::KARAOKE_DOT_BOUNCE_AMPLITUDE
                - 5.0;
            let bottom = line_y + body_h + 5.0;
            overlays.push(KaraokeOverlay {
                copy_rect: PixelRect::from_edges(left, top, right, bottom),
                text_rect,
            });
        }
        overlays
    }

    fn centered_playhead_rect(&self) -> PixelRect {
        PixelRect::from_f32(
            self.center_x - self.playhead_width * 0.5,
            0.0,
            self.playhead_width.max(1.0),
            self.height as f32,
        )
    }

    fn shifted_playhead_rect(&self, height: u32) -> PixelRect {
        PixelRect::from_f32(
            crate::config::playhead_x(0.0, self.width as f32, self.playhead_width),
            0.0,
            self.playhead_width.max(1.0),
            height as f32,
        )
    }
}

#[derive(Clone, Copy)]
struct KaraokeOverlay {
    copy_rect: PixelRect,
    text_rect: PixelRect,
}

#[derive(Clone, Copy)]
struct PixelRect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
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

    fn clipped(self, width: u32, height: u32) -> Self {
        Self {
            left: self.left.clamp(0, width as i32),
            top: self.top.clamp(0, height as i32),
            right: self.right.clamp(0, width as i32),
            bottom: self.bottom.clamp(0, height as i32),
        }
    }
}

fn rgba_height(pixels: &[u8], width: u32) -> Option<u32> {
    let row = width as usize * 4;
    (row > 0 && pixels.len() % row == 0).then_some((pixels.len() / row) as u32)
}

fn crop_rgba(
    source: &[u8],
    source_width: u32,
    output_width: u32,
    height: u32,
    crop_left: i64,
) -> Vec<u8> {
    let mut output = vec![0; output_width as usize * height as usize * 4];
    for pixel in output.chunks_exact_mut(4) {
        pixel.copy_from_slice(&BACKGROUND);
    }
    let source_height = rgba_height(source, source_width).unwrap_or(0).min(height);
    for y in 0..source_height as i64 {
        for x in 0..output_width as i64 {
            let sx = x + crop_left;
            if sx < 0 || sx >= source_width as i64 {
                continue;
            }
            let src = ((y * source_width as i64 + sx) * 4) as usize;
            let dst = ((y * output_width as i64 + x) * 4) as usize;
            output[dst..dst + 4].copy_from_slice(&source[src..src + 4]);
        }
    }
    output
}

fn copy_rect(source: &[u8], destination: &mut [u8], width: u32, height: u32, rect: PixelRect) {
    let rect = rect.clipped(width, height);
    for y in rect.top..rect.bottom {
        let start = ((y as u32 * width + rect.left as u32) * 4) as usize;
        let end = ((y as u32 * width + rect.right as u32) * 4) as usize;
        destination[start..end].copy_from_slice(&source[start..end]);
    }
}

fn copy_intersection(
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
        copy_rect(source, destination, width, height, rect);
    }
}

fn restore_matching_pixels(
    source: &[u8],
    destination: &mut [u8],
    width: u32,
    height: u32,
    area: PixelRect,
    column: PixelRect,
    color: [u8; 4],
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
            if destination[index..index + 4] == color {
                destination[index..index + 4].copy_from_slice(&source[index..index + 4]);
            }
        }
    }
}

fn rects_intersect(a: PixelRect, b: PixelRect) -> bool {
    a.left < b.right && a.right > b.left && a.top < b.bottom && a.bottom > b.top
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crop_moves_the_virtual_center_to_the_requested_playhead() {
        let source_width = 8;
        let output_width = 4;
        let height = 1;
        let mut source = vec![0u8; source_width * 4];
        for x in 0..source_width {
            source[x * 4] = x as u8;
            source[x * 4 + 3] = 255;
        }
        let cropped = crop_rgba(&source, source_width as u32, output_width as u32, height, 3);
        assert_eq!(cropped[0], 3);
        assert_eq!(cropped[12], 6);
    }
}