use std::path::Path;
use std::sync::Arc;
use wgpu::CurrentSurfaceTexture;
use winit::window::Window;

use std::time::Instant;

use crate::command::{Command, CommandHistory, CommandKind};
use crate::graphics::GraphicsContext;
use crate::observer::{TimelineBus, TimelineEvent};
use crate::project::Project;
use crate::ui::renderer::UiRenderer;
use crate::ui::theme;
use crate::ui::widget::{EventResponse, UiEvent};
use crate::ui::Ui;
use crate::video::VideoPlayer;

const SCROLL_DECODE_DELAY_MS: u128 = 100;

pub struct State {
    pub gfx: GraphicsContext,
    ui: Ui,
    ui_renderer: UiRenderer,
    video_player: Option<VideoPlayer>,
    pub project: Project,
    history: CommandHistory,
    pub timeline: TimelineBus,
    last_scroll_time: Option<Instant>,
    scroll_needs_decode: bool,
}

impl State {
    pub async fn new(window: Arc<Window>) -> Self {
        let gfx = GraphicsContext::new(window).await;
        let format = gfx.surface_format();
        let ui_renderer = UiRenderer::new(&gfx.device, &gfx.queue, format);
        let ui = Ui::new(gfx.size.width, gfx.size.height, &ui_renderer.icon_atlas);

        Self {
            gfx, ui, ui_renderer, video_player: None, project: Project::new(),
            history: CommandHistory::new(), timeline: TimelineBus::new(),
            last_scroll_time: None, scroll_needs_decode: false,
        }
    }

    // -- Delegation helpers --

    fn renderer_refs(&self) -> (&wgpu::BindGroupLayout, &wgpu::Sampler) {
        (self.ui_renderer.texture_bind_group_layout(), self.ui_renderer.texture_sampler())
    }

    // -- Public API --

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        self.gfx.resize(new_size);
        self.ui.resize(new_size.width, new_size.height);
    }

    pub fn handle_ui_event(&mut self, event: &UiEvent) -> EventResponse {
        self.ui.handle_event(event, &self.project, self.current_frame(), self.fps())
    }

    pub fn is_editing_text(&self) -> bool {
        self.ui.is_editing_text()
    }

    pub fn request_redraw(&self) {
        self.gfx.request_redraw();
    }

    // -- Video --

    pub fn current_frame(&self) -> i64 {
        self.video_player.as_ref().map_or(0, |p| p.current_frame())
    }

    pub fn fps(&self) -> f64 {
        self.video_player.as_ref().map_or(30.0, |p| p.fps())
    }

    pub fn load_video(&mut self, path: &Path) {
        let (bgl, sampler) = self.renderer_refs();
        let mut player = VideoPlayer::new();
        match player.load(path, &self.gfx.device, &self.gfx.queue, bgl, sampler) {
            Ok(()) => {
                player.set_volume(self.ui.volume());
                let fps = player.fps();
                let total = player.total_frames();
                self.video_player = Some(player);
                self.timeline.emit(TimelineEvent::VideoLoaded { fps, total_frames: total });
                self.timeline.emit(TimelineEvent::FrameChanged { frame: 0 });
            }
            Err(e) => log::error!("Failed to load video: {e}"),
        }
    }

    pub fn toggle_play_pause(&mut self) {
        if let Some(player) = &mut self.video_player {
            player.toggle();
            self.ui.toggle_play_pause();
            if player.is_playing() {
                self.timeline.emit(TimelineEvent::PlaybackStarted);
            } else {
                self.timeline.emit(TimelineEvent::PlaybackStopped);
            }
        }
    }

    pub fn set_volume(&mut self, vol: f32) {
        self.ui.set_volume(vol);
        if let Some(player) = &mut self.video_player {
            player.set_volume(vol);
        }
    }

    pub fn prev_frame(&mut self) {
        let bgl = self.ui_renderer.texture_bind_group_layout();
        let sampler = self.ui_renderer.texture_sampler();
        if let Some(player) = &mut self.video_player {
            player.step_backward(&self.gfx.device, &self.gfx.queue, bgl, sampler);
            if self.ui.is_playing() { self.ui.toggle_play_pause(); }
        }
    }

    pub fn next_frame(&mut self) {
        let bgl = self.ui_renderer.texture_bind_group_layout();
        let sampler = self.ui_renderer.texture_sampler();
        if let Some(player) = &mut self.video_player {
            player.step_forward(&self.gfx.device, &self.gfx.queue, bgl, sampler);
            if self.ui.is_playing() { self.ui.toggle_play_pause(); }
        }
    }

    pub fn seek_relative(&mut self, delta: i32) {
        if let Some(player) = &mut self.video_player {
            player.seek_frame_instant(delta);
            self.timeline.emit(TimelineEvent::FrameChanged { frame: player.current_frame() });
        }
        self.last_scroll_time = Some(Instant::now());
        self.scroll_needs_decode = true;
    }

    fn tick_scroll_decode(&mut self) {
        if !self.scroll_needs_decode { return; }
        if let Some(t) = self.last_scroll_time {
            if t.elapsed().as_millis() >= SCROLL_DECODE_DELAY_MS {
                self.scroll_needs_decode = false;
                let bgl = self.ui_renderer.texture_bind_group_layout();
                let sampler = self.ui_renderer.texture_sampler();
                if let Some(player) = &mut self.video_player {
                    player.decode_current_frame(&self.gfx.device, &self.gfx.queue, bgl, sampler);
                }
            }
        }
    }

    // -- Undo / Redo --

    pub fn undo(&mut self) {
        self.history.undo(&mut self.project);
    }

    pub fn redo(&mut self) {
        self.history.redo(&mut self.project);
    }

    // -- Project / Lines (all via Command pattern) --

    pub fn open_toolbar_dropdown(&mut self, dropdown: crate::ui::widget::ToolbarDropdown) {
        self.ui.toggle_toolbar_dropdown(dropdown);
    }

    pub fn add_marker(&mut self, kind: crate::rythmo_line::MarkerKind) {
        let frame = self.current_frame();
        self.project.markers.push(crate::rythmo_line::RythmoMarker { kind, frame });
        self.history.push(Command::AddMarker { index: self.project.markers.len() - 1 });
    }

    pub fn add_quick_line(&mut self, text: String) {
        let frame = self.current_frame();
        let dur = (self.fps() * 1.0) as i64; // 1 second
        let line_id = self.project.add_line(frame, dur, 0.0);
        if let Some(line) = self.project.get_line_mut(line_id) {
            line.text = text;
        }
        self.history.push(Command::CreateLine { line_id });
    }

    pub fn create_line(&mut self, frame: i64, y_slot: f32) {
        let dur = (self.fps() * theme::DEFAULT_LINE_DURATION_SEC) as i64;
        let line_id = self.project.add_line(frame, dur, y_slot);
        self.history.push(Command::CreateLine { line_id });
    }

    pub fn move_line(&mut self, id: u64, start_frame: i64, y_slot: f32) {
        // Coalesce: update last command if same line drag
        if self.history.last_matches(id, CommandKind::MoveLine) {
            if let Some(line) = self.project.get_line_mut(id) {
                line.start_frame = start_frame;
                line.y_slot = y_slot;
            }
            self.history.update_last(|cmd| {
                if let Command::MoveLine { new_start, new_y_slot, .. } = cmd {
                    *new_start = start_frame;
                    *new_y_slot = y_slot;
                }
            });
        } else if let Some(line) = self.project.get_line(id) {
            let old_start = line.start_frame;
            let old_y = line.y_slot;
            if let Some(l) = self.project.get_line_mut(id) {
                l.start_frame = start_frame;
                l.y_slot = y_slot;
            }
            self.history.push(Command::MoveLine {
                line_id: id, old_start, old_y_slot: old_y, new_start: start_frame, new_y_slot: y_slot,
            });
        }
    }

    pub fn resize_line(&mut self, id: u64, start_frame: i64, duration_frames: i64) {
        if self.history.last_matches(id, CommandKind::ResizeLine) {
            if let Some(l) = self.project.get_line_mut(id) {
                l.start_frame = start_frame;
                l.duration_frames = duration_frames;
            }
            self.history.update_last(|cmd| {
                if let Command::ResizeLine { new_start, new_dur, .. } = cmd {
                    *new_start = start_frame;
                    *new_dur = duration_frames;
                }
            });
        } else if let Some(line) = self.project.get_line(id) {
            let old_start = line.start_frame;
            let old_dur = line.duration_frames;
            if let Some(l) = self.project.get_line_mut(id) {
                l.start_frame = start_frame;
                l.duration_frames = duration_frames;
            }
            self.history.push(Command::ResizeLine {
                line_id: id, old_start, old_dur, new_start: start_frame, new_dur: duration_frames,
            });
        }
    }

    pub fn update_line_text(&mut self, id: u64, text: String) {
        // Coalesce: update last text command for same line
        if self.history.last_matches(id, CommandKind::UpdateLineText) {
            if let Some(l) = self.project.get_line_mut(id) {
                l.text = text.clone();
            }
            self.history.update_last(|cmd| {
                if let Command::UpdateLineText { new_text, .. } = cmd {
                    *new_text = text;
                }
            });
        } else {
            let old_text = self.project.get_line(id).map(|l| l.text.clone()).unwrap_or_default();
            if let Some(l) = self.project.get_line_mut(id) {
                l.text = text.clone();
            }
            self.history.push(Command::UpdateLineText { line_id: id, old_text, new_text: text });
        }
    }

    pub fn set_character(&mut self, line_id: u64, name: String, color: [f32; 4]) {
        let (old_name, old_color) = self.project.get_line(line_id)
            .map(|l| (l.character_name.clone(), l.character_color))
            .unwrap_or_default();
        self.project.set_character(line_id, name.clone(), color);
        self.history.push(Command::SetCharacter {
            line_id, old_name, old_color, new_name: name, new_color: color,
        });
    }

    pub fn set_character_color(&mut self, line_id: u64, color: [f32; 4]) {
        if self.history.last_matches(line_id, CommandKind::SetCharacterColor) {
            if let Some(l) = self.project.get_line_mut(line_id) {
                l.character_color = color;
            }
            self.history.update_last(|cmd| {
                if let Command::SetCharacterColor { new_color, .. } = cmd {
                    *new_color = color;
                }
            });
        } else {
            let old_color = self.project.get_line(line_id).map(|l| l.character_color).unwrap_or_default();
            if let Some(l) = self.project.get_line_mut(line_id) {
                l.character_color = color;
            }
            self.history.push(Command::SetCharacterColor { line_id, old_color, new_color: color });
        }
    }

    pub fn update_character_name(&mut self, line_id: u64, name: String) {
        let known_color = self.project.known_characters.iter()
            .find(|c| c.name == name)
            .map(|c| c.color);

        // Coalesce character name edits
        if self.history.last_matches(line_id, CommandKind::SetCharacter) {
            if let Some(l) = self.project.get_line_mut(line_id) {
                l.character_name = name.clone();
                if let Some(c) = known_color { l.character_color = c; }
            }
            let final_color = self.project.get_line(line_id).map(|l| l.character_color).unwrap_or_default();
            self.history.update_last(|cmd| {
                if let Command::SetCharacter { new_name, new_color, .. } = cmd {
                    *new_name = name;
                    *new_color = final_color;
                }
            });
        } else {
            let (old_name, old_color) = self.project.get_line(line_id)
                .map(|l| (l.character_name.clone(), l.character_color))
                .unwrap_or_default();
            if let Some(l) = self.project.get_line_mut(line_id) {
                l.character_name = name.clone();
                if let Some(c) = known_color { l.character_color = c; }
            }
            let final_color = self.project.get_line(line_id).map(|l| l.character_color).unwrap_or_default();
            self.history.push(Command::SetCharacter {
                line_id, old_name, old_color, new_name: name, new_color: final_color,
            });
        }
    }

    pub fn finalize_character(&mut self, line_id: u64) {
        let (name, color) = match self.project.get_line(line_id) {
            Some(l) if !l.character_name.is_empty() => (l.character_name.clone(), l.character_color),
            _ => return,
        };
        self.project.set_character(line_id, name, color);
    }

    // -- Render --

    pub fn render(&mut self) {
        let surface_texture = match self.gfx.surface.get_current_texture() {
            CurrentSurfaceTexture::Success(tex) | CurrentSurfaceTexture::Suboptimal(tex) => tex,
            CurrentSurfaceTexture::Outdated | CurrentSurfaceTexture::Lost => {
                self.gfx.surface.configure(&self.gfx.device, &self.gfx.config);
                return;
            }
            CurrentSurfaceTexture::Timeout | CurrentSurfaceTexture::Occluded => return,
            _ => return,
        };

        let view = surface_texture.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self.gfx.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });

        // Clear
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Clear Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view, resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None, timestamp_writes: None,
                occlusion_query_set: None, multiview_mask: None,
            });
        }

        // Drain timeline events (observers already received them via current_frame param)
        let _events = self.timeline.drain();

        // Debounced scroll decode
        self.tick_scroll_decode();

        // Video tick — emit FrameChanged so observers (rythmo) stay in sync
        if let Some(player) = &mut self.video_player {
            let prev_frame = player.current_frame();
            let (bgl, sampler) = (
                self.ui_renderer.texture_bind_group_layout(),
                self.ui_renderer.texture_sampler(),
            );
            player.tick(&self.gfx.device, &self.gfx.queue, bgl, sampler);
            if player.current_frame() != prev_frame {
                self.timeline.emit(TimelineEvent::FrameChanged { frame: player.current_frame() });
            }
            if !player.is_playing() && self.ui.is_playing() {
                self.timeline.emit(TimelineEvent::PlaybackStopped);
                self.ui.toggle_play_pause();
            }
        }

        // Video quad
        let video_quad = build_video_quad(&self.video_player, &self.ui);
        let current_frame = self.current_frame();

        // UI render
        self.ui.render(
            &mut self.ui_renderer,
            &self.gfx.device, &self.gfx.queue, &mut encoder, &view,
            self.gfx.config.width, self.gfx.config.height,
            video_quad.as_ref().map(|(bg, inst)| (*bg, *inst)),
            &self.project, current_frame,
        );

        self.gfx.queue.submit(std::iter::once(encoder.finish()));
        surface_texture.present();
    }
}

fn build_video_quad<'a>(
    video_player: &'a Option<VideoPlayer>,
    ui: &Ui,
) -> Option<(&'a wgpu::BindGroup, crate::ui::widget::IconInstance)> {
    let player = video_player.as_ref()?;
    let bind_group = player.bind_group.as_ref()?;
    let (vid_w, vid_h) = player.video_size()?;
    let preview = &ui.layout().video_preview;

    let vid_aspect = vid_w as f32 / vid_h as f32;
    let zone_aspect = preview.width / preview.height;
    let (draw_w, draw_h) = if vid_aspect > zone_aspect {
        (preview.width, preview.width / vid_aspect)
    } else {
        (preview.height * vid_aspect, preview.height)
    };

    Some((
        bind_group,
        crate::ui::widget::IconInstance {
            rect: [preview.x + (preview.width - draw_w) / 2.0, preview.y + (preview.height - draw_h) / 2.0, draw_w, draw_h],
            uv_rect: [0.0, 0.0, 1.0, 1.0],
            tint: [1.0, 1.0, 1.0, 1.0],
        },
    ))
}
