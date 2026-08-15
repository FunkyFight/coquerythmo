//! Backend-neutral scene and pointer interaction for the Voicelines workspace.

use crate::ui::focus::AccessibleRole;
use crate::ui::primitives::{
    EventResponse, HAlign, Overflow, QuadInstance, Rect, UiAction, UiEvent, VAlign,
};
use crate::voicelines::{Audio, Region, RegionId, VoicelinesProject};

const SIDEBAR_W: f32 = 270.0;
const HEADER_H: f32 = 54.0;
const TOOLBAR_H: f32 = 42.0;
const REGIONS_H: f32 = 150.0;
const AUDIO_ROW_H: f32 = 46.0;
const REGION_ROW_H: f32 = 30.0;
const HANDLE_W: f32 = 7.0;
const AUDIO_MENU_W: f32 = 220.0;
const AUDIO_MENU_ITEM_H: f32 = 34.0;

const BG: [f32; 4] = [0.055, 0.058, 0.072, 1.0];
const PANEL: [f32; 4] = [0.085, 0.09, 0.11, 1.0];
const PANEL_ALT: [f32; 4] = [0.115, 0.12, 0.15, 1.0];
const BORDER: [f32; 4] = [0.24, 0.25, 0.31, 0.9];
const ACCENT: [f32; 4] = [0.38, 0.31, 0.88, 1.0];
const TEXT: [u8; 3] = [228, 230, 238];
const MUTED: [u8; 3] = [150, 154, 170];

#[derive(Debug, Clone, Copy, Default)]
pub struct VoicelinesLayout {
    pub content: Rect,
    pub sidebar: Rect,
    pub header: Rect,
    pub toolbar: Rect,
    pub waveform: Rect,
    pub regions: Rect,
}

impl VoicelinesLayout {
    pub fn compute(content: Rect) -> Self {
        let sidebar_width = SIDEBAR_W.min((content.width * 0.34).max(190.0));
        let sidebar = Rect {
            x: content.x,
            y: content.y,
            width: sidebar_width,
            height: content.height,
        };
        let main_x = sidebar.x + sidebar.width;
        let main_width = (content.x + content.width - main_x).max(0.0);
        let header = Rect {
            x: main_x,
            y: content.y,
            width: main_width,
            height: HEADER_H,
        };
        let toolbar = Rect {
            x: main_x,
            y: header.y + header.height,
            width: main_width,
            height: TOOLBAR_H,
        };
        let regions_height = REGIONS_H.min((content.height * 0.28).max(96.0));
        let regions = Rect {
            x: main_x,
            y: content.y + content.height - regions_height,
            width: main_width,
            height: regions_height,
        };
        let waveform = Rect {
            x: main_x + 12.0,
            y: toolbar.y + toolbar.height + 12.0,
            width: (main_width - 24.0).max(0.0),
            height: (regions.y - toolbar.y - toolbar.height - 24.0).max(0.0),
        };
        Self {
            content,
            sidebar,
            header,
            toolbar,
            waveform,
            regions,
        }
    }

    fn buttons(self) -> [(Rect, HeaderAction); 5] {
        let gap = 6.0;
        let y = self.header.y + 9.0;
        let height = self.header.height - 18.0;
        let widths = [118.0, 106.0, 184.0, 92.0, 92.0];
        let scale =
            ((self.header.width - 20.0 - gap * 4.0).max(0.0) / widths.iter().sum::<f32>()).min(1.0);
        let actions = [
            HeaderAction::Import,
            HeaderAction::Detect,
            HeaderAction::Naming,
            HeaderAction::Save,
            HeaderAction::Load,
        ];
        let mut x = self.header.x + 10.0;
        std::array::from_fn(|index| {
            let rect = Rect {
                x,
                y,
                width: widths[index] * scale,
                height,
            };
            x += widths[index] * scale + gap;
            (rect, actions[index])
        })
    }
}

#[derive(Debug, Clone, Copy)]
enum HeaderAction {
    Import,
    Detect,
    Naming,
    Save,
    Load,
}

#[derive(Debug, Clone)]
pub struct SceneLabel {
    pub text: String,
    pub bounds: Rect,
    pub h_align: HAlign,
    pub font_size: f32,
    pub color: [u8; 3],
}

#[derive(Debug, Clone)]
pub struct SceneControl {
    pub id: String,
    pub label: String,
    pub bounds: Rect,
    pub role: AccessibleRole,
    pub selected: bool,
}

#[derive(Debug, Clone, Default)]
pub struct VoicelinesScene {
    pub quads: Vec<QuadInstance>,
    pub labels: Vec<SceneLabel>,
    pub system_quads: Vec<QuadInstance>,
    pub system_labels: Vec<SceneLabel>,
    pub controls: Vec<SceneControl>,
}

#[derive(Debug, Clone, Copy)]
enum Drag {
    Create {
        anchor_ms: u64,
        current_ms: u64,
    },
    Move {
        region_id: RegionId,
        anchor_pointer_ms: u64,
        current_ms: u64,
        start_ms: u64,
        end_ms: u64,
    },
    Start {
        region_id: RegionId,
        end_ms: u64,
        current_ms: u64,
    },
    End {
        region_id: RegionId,
        start_ms: u64,
        current_ms: u64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AudioMenuMode {
    Send,
    Update,
}

#[derive(Debug, Clone, Copy)]
struct AudioContextMenu {
    audio_id: u64,
    x: f32,
    y: f32,
    open_mode: Option<AudioMenuMode>,
    hover_parent: Option<AudioMenuMode>,
    hover_target: Option<crate::application::workspace_service::WorkspaceId>,
    delivered_comic_dubs: bool,
    delivered_recording: bool,
}

#[derive(Debug, Default)]
pub struct VoicelinesWorkspaceUi {
    view_start_ms: u64,
    view_duration_ms: u64,
    audio_scroll: usize,
    selected_regions: Vec<RegionId>,
    region_scroll: usize,
    dragging_region_scrollbar: bool,
    region_scrollbar_drag_offset: f32,
    drag: Option<Drag>,
    rename: Option<(RegionId, String)>,
    naming_pattern: Option<String>,
    audio_context_menu: Option<AudioContextMenu>,
    recording_transfer_disabled: bool,
}

impl VoicelinesWorkspaceUi {
    pub fn is_editing_text(&self) -> bool {
        self.rename.is_some() || self.naming_pattern.is_some()
    }

    pub fn set_selected_region(&mut self, selected: Option<RegionId>) {
        self.selected_regions = selected.into_iter().collect();
    }

    pub fn selected_regions(&self) -> &[RegionId] {
        &self.selected_regions
    }

    pub fn set_recording_transfer_disabled(&mut self, disabled: bool) {
        self.recording_transfer_disabled = disabled;
        if disabled {
            if let Some(menu) = &mut self.audio_context_menu {
                if menu.hover_target
                    == Some(crate::application::workspace_service::WorkspaceId::Recording)
                {
                    menu.hover_target =
                        Some(crate::application::workspace_service::WorkspaceId::ComicDubs);
                }
            }
        }
    }

    pub fn begin_rename(&mut self, region_id: RegionId, name: String) {
        self.naming_pattern = None;
        self.selected_regions = vec![region_id];
        self.rename = Some((region_id, name));
    }

    pub fn begin_naming_pattern(&mut self, pattern: String) {
        self.rename = None;
        self.naming_pattern = Some(pattern);
    }

    pub fn audio_selected(&mut self, duration_ms: u64, audio_index: usize) {
        self.view_start_ms = 0;
        self.view_duration_ms = duration_ms.clamp(1, 10_000);
        self.audio_scroll = audio_index.saturating_sub(3);
        self.selected_regions.clear();
        self.region_scroll = 0;
        self.drag = None;
        self.rename = None;
    }

    pub fn sync(&mut self, project: &VoicelinesProject, layout: VoicelinesLayout) {
        let Some(audio) = project.active_audio() else {
            self.audio_selected(1, 0);
            return;
        };
        let duration = audio.duration_ms().max(1);
        if self.view_duration_ms == 0 {
            let audio_index = project
                .audios()
                .iter()
                .position(|candidate| candidate.id == audio.id)
                .unwrap_or(0);
            self.audio_selected(duration, audio_index);
        }
        self.view_duration_ms = self.view_duration_ms.clamp(100, duration);
        self.view_start_ms = self
            .view_start_ms
            .min(duration.saturating_sub(self.view_duration_ms));
        self.selected_regions
            .retain(|id| audio.regions.iter().any(|region| region.id == *id));
        let visible = visible_region_rows(layout);
        let max_scroll = audio.regions.len().saturating_sub(visible);
        self.region_scroll = self.region_scroll.min(max_scroll);
    }

    pub fn handle_event(
        &mut self,
        event: &UiEvent,
        project: &VoicelinesProject,
        layout: VoicelinesLayout,
    ) -> EventResponse {
        self.sync(project, layout);
        if let Some(response) = self.handle_audio_context_menu(event, project, layout) {
            return response;
        }
        if let Some(response) = self.handle_naming_pattern(event) {
            return response;
        }
        if let Some(response) = self.handle_rename(event) {
            return response;
        }

        if let UiEvent::MousePress { x, y } = event {
            for (rect, action) in layout.buttons() {
                if rect.contains(*x, *y) {
                    if matches!(action, HeaderAction::Naming) {
                        self.begin_naming_pattern(editable_automatic_name(
                            project.automatic_pattern(),
                        ));
                        return EventResponse::Consumed;
                    }
                    return EventResponse::Action(match action {
                        HeaderAction::Import => UiAction::VoicelinesImportAudio,
                        HeaderAction::Detect => UiAction::VoicelinesAutoDetect,
                        HeaderAction::Naming => unreachable!(),
                        HeaderAction::Save => UiAction::QuickSave,
                        HeaderAction::Load => UiAction::VoicelinesLoadSession,
                    });
                }
            }
        }

        let Some(audio) = project.active_audio() else {
            if matches!(event, UiEvent::MousePress { x, y } if layout.content.contains(*x, *y)) {
                return EventResponse::Action(UiAction::VoicelinesImportAudio);
            }
            return EventResponse::Ignored;
        };

        if self.handle_region_scrollbar(audio.regions.len(), layout, event) {
            return EventResponse::Consumed;
        }

        match event {
            UiEvent::MousePress { x, y } => {
                if let Some((id, remove)) = audio_hit(project, layout, self.audio_scroll, *x, *y) {
                    return EventResponse::Action(if remove {
                        UiAction::VoicelinesRemoveAudio(id)
                    } else {
                        UiAction::VoicelinesSelectAudio(id)
                    });
                }
                if let Some(region) = region_row_hit(audio, layout, self.region_scroll, *x, *y) {
                    self.selected_regions = vec![region.id];
                    return EventResponse::Consumed;
                }
                if layout.waveform.contains(*x, *y) {
                    let pointer_ms = self.ms_at_x(layout, *x, audio.duration_ms());
                    if let Some((region, part)) = region_hit(
                        audio,
                        layout.waveform,
                        self.view_start_ms,
                        self.view_duration_ms,
                        *x,
                    ) {
                        self.selected_regions = vec![region.id];
                        self.drag = Some(match part {
                            RegionPart::Start => Drag::Start {
                                region_id: region.id,
                                end_ms: region.end_ms,
                                current_ms: pointer_ms,
                            },
                            RegionPart::End => Drag::End {
                                region_id: region.id,
                                start_ms: region.start_ms,
                                current_ms: pointer_ms,
                            },
                            RegionPart::Body => Drag::Move {
                                region_id: region.id,
                                anchor_pointer_ms: pointer_ms,
                                current_ms: pointer_ms,
                                start_ms: region.start_ms,
                                end_ms: region.end_ms,
                            },
                        });
                    } else {
                        self.selected_regions.clear();
                        self.drag = Some(Drag::Create {
                            anchor_ms: pointer_ms,
                            current_ms: pointer_ms,
                        });
                    }
                    return EventResponse::Consumed;
                }
            }
            UiEvent::MouseMove { x, .. } => {
                if let Some(drag) = self.drag.as_mut() {
                    let current = ms_at_x(
                        layout.waveform,
                        self.view_start_ms,
                        self.view_duration_ms,
                        *x,
                        audio.duration_ms(),
                    );
                    match drag {
                        Drag::Create { current_ms, .. }
                        | Drag::Start { current_ms, .. }
                        | Drag::End { current_ms, .. } => *current_ms = current,
                        Drag::Move { current_ms, .. } => *current_ms = current,
                    }
                    return EventResponse::Consumed;
                }
            }
            UiEvent::MouseRelease { .. } => {
                if let Some(drag) = self.drag.take() {
                    return finish_drag(drag, audio.duration_ms());
                }
            }
            UiEvent::DoubleClick { x, y } => {
                let region =
                    region_row_hit(audio, layout, self.region_scroll, *x, *y).or_else(|| {
                        region_hit(
                            audio,
                            layout.waveform,
                            self.view_start_ms,
                            self.view_duration_ms,
                            *x,
                        )
                        .map(|(region, _)| region)
                    });
                if let Some(region) = region {
                    self.selected_regions = vec![region.id];
                    self.rename = Some((region.id, region.name.clone()));
                    return EventResponse::Consumed;
                }
            }
            UiEvent::ShiftMousePress { x, y } => {
                let region =
                    region_row_hit(audio, layout, self.region_scroll, *x, *y).or_else(|| {
                        layout
                            .waveform
                            .contains(*x, *y)
                            .then(|| {
                                region_hit(
                                    audio,
                                    layout.waveform,
                                    self.view_start_ms,
                                    self.view_duration_ms,
                                    *x,
                                )
                                .map(|(region, _)| region)
                            })
                            .flatten()
                    });
                if let Some(region) = region {
                    return EventResponse::Action(UiAction::VoicelinesExportRegion(region.id));
                }
            }
            UiEvent::CtrlClick { x, y } => {
                let region = region_row_hit(audio, layout, self.region_scroll, *x, *y).or_else(|| {
                    layout
                        .waveform
                        .contains(*x, *y)
                        .then(|| {
                            region_hit(
                                audio,
                                layout.waveform,
                                self.view_start_ms,
                                self.view_duration_ms,
                                *x,
                            )
                            .map(|(region, _)| region)
                        })
                        .flatten()
                });
                if let Some(region) = region {
                    if let Some(index) = self
                        .selected_regions
                        .iter()
                        .position(|id| *id == region.id)
                    {
                        self.selected_regions.remove(index);
                    } else {
                        self.selected_regions.push(region.id);
                    }
                    return EventResponse::Consumed;
                }
            }
            UiEvent::ContextMenu { x, y } => {
                let region =
                    region_row_hit(audio, layout, self.region_scroll, *x, *y).or_else(|| {
                        if layout.waveform.contains(*x, *y) {
                            region_hit(
                                audio,
                                layout.waveform,
                                self.view_start_ms,
                                self.view_duration_ms,
                                *x,
                            )
                            .map(|(region, _)| region)
                        } else {
                            None
                        }
                    });
                if let Some(region) = region {
                    return EventResponse::Action(UiAction::VoicelinesPlayRegion(region.id));
                }
            }
            event
                if matches!(event, UiEvent::Delete)
                    || matches!(event, UiEvent::KeyInput { text } if text == "\x7f") =>
            {
                if let Some(region_id) = self.selected_regions.last().copied() {
                    return EventResponse::Action(UiAction::VoicelinesDeleteRegion(region_id));
                }
            }
            UiEvent::Scroll {
                x, y, delta, ctrl, ..
            } if layout.waveform.contains(*x, *y) => {
                let duration = audio.duration_ms().max(1);
                if *ctrl {
                    let anchor = self.ms_at_x(layout, *x, duration);
                    let factor = if *delta > 0.0 { 0.75 } else { 1.35 };
                    let next = (self.view_duration_ms as f64 * factor).round() as u64;
                    let next = next.clamp(100, duration);
                    let ratio = ((*x - layout.waveform.x) / layout.waveform.width.max(1.0))
                        .clamp(0.0, 1.0) as f64;
                    self.view_duration_ms = next;
                    self.view_start_ms = anchor
                        .saturating_sub((next as f64 * ratio).round() as u64)
                        .min(duration.saturating_sub(next));
                } else {
                    let amount = (self.view_duration_ms / 8).max(1);
                    self.view_start_ms = if *delta > 0.0 {
                        self.view_start_ms.saturating_sub(amount)
                    } else {
                        self.view_start_ms
                            .saturating_add(amount)
                            .min(duration.saturating_sub(self.view_duration_ms))
                    };
                }
                return EventResponse::Consumed;
            }
            UiEvent::Scroll { x, y, delta, .. } if layout.regions.contains(*x, *y) => {
                let max_scroll = audio
                    .regions
                    .len()
                    .saturating_sub(visible_region_rows(layout));
                self.region_scroll = if *delta > 0.0 {
                    self.region_scroll.saturating_sub(1)
                } else {
                    self.region_scroll.saturating_add(1).min(max_scroll)
                };
                return EventResponse::Consumed;
            }
            UiEvent::Scroll { x, y, delta, .. } if layout.sidebar.contains(*x, *y) => {
                let max_scroll = project
                    .audios()
                    .len()
                    .saturating_sub(visible_audio_rows(layout));
                self.audio_scroll = if *delta > 0.0 {
                    self.audio_scroll.saturating_sub(1)
                } else {
                    self.audio_scroll.saturating_add(1).min(max_scroll)
                };
                return EventResponse::Consumed;
            }
            _ => {}
        }
        EventResponse::Ignored
    }

    fn handle_audio_context_menu(
        &mut self,
        event: &UiEvent,
        project: &VoicelinesProject,
        layout: VoicelinesLayout,
    ) -> Option<EventResponse> {
        if let UiEvent::ContextMenu { x, y } = event {
            let Some((audio_id, _)) = audio_hit(project, layout, self.audio_scroll, *x, *y) else {
                self.audio_context_menu = None;
                return None;
            };
            let audio = project.audio(audio_id).expect("hit-tested audio");
            let (x, y) = super::context_menu::clamped_origin(
                *x,
                *y,
                AUDIO_MENU_W * 2.0 - 2.0,
                AUDIO_MENU_ITEM_H * 2.0,
                layout.content.x + layout.content.width,
                layout.content.y + layout.content.height,
            );
            self.audio_context_menu = Some(AudioContextMenu {
                audio_id,
                x,
                y,
                open_mode: None,
                hover_parent: None,
                hover_target: None,
                delivered_comic_dubs: audio.has_delivery(
                    crate::voicelines::DeliveryDestination::ComicDubs,
                ),
                delivered_recording: audio.has_delivery(
                    crate::voicelines::DeliveryDestination::Recording,
                ),
            });
            return Some(EventResponse::Consumed);
        }

        let recording_transfer_disabled = self.recording_transfer_disabled;
        let menu = self.audio_context_menu.as_mut()?;
        let send_parent = Rect {
            x: menu.x,
            y: menu.y,
            width: AUDIO_MENU_W,
            height: AUDIO_MENU_ITEM_H,
        };
        let has_update = menu.delivered_comic_dubs || menu.delivered_recording;
        let delivered_comic_dubs = menu.delivered_comic_dubs;
        let delivered_recording = menu.delivered_recording;
        let update_parent = Rect {
            y: menu.y + AUDIO_MENU_ITEM_H,
            ..send_parent
        };
        let submenu = Rect {
            x: menu.x + AUDIO_MENU_W - 2.0,
            height: AUDIO_MENU_ITEM_H * 2.0,
            ..send_parent
        };
        let target_at = |mode, x, y| {
            if !submenu.contains(x, y) {
                return None;
            }
            if y < submenu.y + AUDIO_MENU_ITEM_H {
                (!matches!(mode, AudioMenuMode::Update) || delivered_comic_dubs)
                    .then_some(crate::application::workspace_service::WorkspaceId::ComicDubs)
            } else if !recording_transfer_disabled
                && (!matches!(mode, AudioMenuMode::Update) || delivered_recording)
            {
                Some(crate::application::workspace_service::WorkspaceId::Recording)
            } else {
                None
            }
        };
        let first_target = |mode| {
            if !matches!(mode, AudioMenuMode::Update) || delivered_comic_dubs {
                crate::application::workspace_service::WorkspaceId::ComicDubs
            } else {
                crate::application::workspace_service::WorkspaceId::Recording
            }
        };
        match event {
            UiEvent::MouseMove { x, y } => {
                menu.hover_parent = if send_parent.contains(*x, *y) {
                    Some(AudioMenuMode::Send)
                } else if has_update && update_parent.contains(*x, *y) {
                    Some(AudioMenuMode::Update)
                } else {
                    None
                };
                if let Some(mode) = menu.hover_parent {
                    menu.open_mode = Some(mode);
                }
                menu.hover_target = menu.open_mode.and_then(|mode| target_at(mode, *x, *y));
                Some(EventResponse::Consumed)
            }
            UiEvent::MousePress { x, y } if menu.open_mode.is_some() => {
                let mode = menu.open_mode.unwrap();
                if let Some(workspace) = target_at(mode, *x, *y) {
                    let audio_id = menu.audio_id;
                    self.audio_context_menu = None;
                    let action = match mode {
                        AudioMenuMode::Send => UiAction::VoicelinesSendAudio {
                            audio_id,
                            workspace,
                        },
                        AudioMenuMode::Update => UiAction::VoicelinesUpdateAudio {
                            audio_id,
                            workspace,
                        },
                    };
                    return Some(EventResponse::Action(action));
                }
                if send_parent.contains(*x, *y) {
                    menu.open_mode = Some(AudioMenuMode::Send);
                    menu.hover_target = Some(first_target(AudioMenuMode::Send));
                    return Some(EventResponse::Consumed);
                }
                if has_update && update_parent.contains(*x, *y) {
                    menu.open_mode = Some(AudioMenuMode::Update);
                    menu.hover_target = Some(first_target(AudioMenuMode::Update));
                    return Some(EventResponse::Consumed);
                }
                self.audio_context_menu = None;
                Some(EventResponse::Consumed)
            }
            UiEvent::MousePress { x, y } if send_parent.contains(*x, *y) => {
                menu.open_mode = Some(AudioMenuMode::Send);
                menu.hover_target = Some(first_target(AudioMenuMode::Send));
                Some(EventResponse::Consumed)
            }
            UiEvent::MousePress { x, y } if has_update && update_parent.contains(*x, *y) => {
                menu.open_mode = Some(AudioMenuMode::Update);
                menu.hover_target = Some(first_target(AudioMenuMode::Update));
                Some(EventResponse::Consumed)
            }
            UiEvent::CursorRight => {
                let mode = menu.hover_parent.unwrap_or(AudioMenuMode::Send);
                menu.open_mode = Some(mode);
                menu.hover_target = Some(first_target(mode));
                Some(EventResponse::Consumed)
            }
            UiEvent::CursorUp | UiEvent::CursorDown if menu.open_mode.is_some() => {
                let mode = menu.open_mode.unwrap();
                let comic_enabled = !matches!(mode, AudioMenuMode::Update)
                    || delivered_comic_dubs;
                let recording_enabled = !recording_transfer_disabled
                    && (!matches!(mode, AudioMenuMode::Update) || delivered_recording);
                menu.hover_target = Some(if comic_enabled && recording_enabled {
                    match menu.hover_target {
                        Some(crate::application::workspace_service::WorkspaceId::ComicDubs) => {
                            crate::application::workspace_service::WorkspaceId::Recording
                        }
                        _ => crate::application::workspace_service::WorkspaceId::ComicDubs,
                    }
                } else {
                    first_target(mode)
                });
                Some(EventResponse::Consumed)
            }
            UiEvent::Activate if menu.open_mode.is_some() => {
                let mode = menu.open_mode.unwrap();
                let audio_id = menu.audio_id;
                let workspace = menu.hover_target.unwrap_or_else(|| first_target(mode));
                self.audio_context_menu = None;
                Some(EventResponse::Action(match mode {
                    AudioMenuMode::Send => UiAction::VoicelinesSendAudio {
                        audio_id,
                        workspace,
                    },
                    AudioMenuMode::Update => UiAction::VoicelinesUpdateAudio {
                        audio_id,
                        workspace,
                    },
                }))
            }
            UiEvent::KeyInput { text } if text == "\x1b" => {
                self.audio_context_menu = None;
                Some(EventResponse::Consumed)
            }
            UiEvent::MousePress { .. } | UiEvent::OpenContextMenu => {
                self.audio_context_menu = None;
                Some(EventResponse::Consumed)
            }
            _ => Some(EventResponse::Consumed),
        }
    }

    fn handle_region_scrollbar(
        &mut self,
        region_count: usize,
        layout: VoicelinesLayout,
        event: &UiEvent,
    ) -> bool {
        let Some((track, thumb, max_scroll)) =
            region_scrollbar_geometry(layout, region_count, self.region_scroll)
        else {
            self.dragging_region_scrollbar = false;
            return false;
        };
        match event {
            UiEvent::MousePress { x, y } if thumb.contains(*x, *y) => {
                self.dragging_region_scrollbar = true;
                self.region_scrollbar_drag_offset = *y - thumb.y;
                true
            }
            UiEvent::MousePress { x, y } if track.contains(*x, *y) => {
                let travel = (track.height - thumb.height).max(1.0);
                self.region_scroll = (((*y - track.y - thumb.height / 2.0) / travel)
                    .clamp(0.0, 1.0)
                    * max_scroll as f32)
                    .round() as usize;
                self.dragging_region_scrollbar = true;
                self.region_scrollbar_drag_offset = thumb.height / 2.0;
                true
            }
            UiEvent::MouseMove { y, .. } if self.dragging_region_scrollbar => {
                let travel = (track.height - thumb.height).max(1.0);
                self.region_scroll = (((*y - self.region_scrollbar_drag_offset - track.y) / travel)
                    .clamp(0.0, 1.0)
                    * max_scroll as f32)
                    .round() as usize;
                true
            }
            UiEvent::MouseRelease { .. } if self.dragging_region_scrollbar => {
                self.dragging_region_scrollbar = false;
                true
            }
            _ => false,
        }
    }

    pub fn scene(
        &self,
        project: &VoicelinesProject,
        current_ms: u64,
        layout: VoicelinesLayout,
    ) -> VoicelinesScene {
        let mut scene = VoicelinesScene::default();
        scene.quads.push(quad(layout.content, BG, [0.0; 4], 0.0));
        scene.quads.push(quad(layout.sidebar, PANEL, BORDER, 0.0));
        scene.quads.push(quad(layout.header, PANEL, BORDER, 0.0));
        scene.quads.push(quad(layout.regions, PANEL, BORDER, 0.0));
        label(
            &mut scene,
            "Audios",
            Rect {
                x: layout.sidebar.x + 14.0,
                y: layout.sidebar.y + 10.0,
                width: layout.sidebar.width - 28.0,
                height: 26.0,
            },
            HAlign::Left,
            17.0,
            TEXT,
        );

        for (row, audio) in project
            .audios()
            .iter()
            .skip(self.audio_scroll)
            .take(visible_audio_rows(layout))
            .enumerate()
        {
            let rect = audio_row_rect(layout, row);
            let selected = project.active_audio_id() == Some(audio.id);
            scene.quads.push(quad(
                rect,
                if selected {
                    [0.16, 0.14, 0.30, 1.0]
                } else {
                    PANEL_ALT
                },
                if selected { ACCENT } else { BORDER },
                7.0,
            ));
            label(
                &mut scene,
                &audio.file_name,
                inset(rect, 10.0, 4.0),
                HAlign::Left,
                13.0,
                TEXT,
            );
            label(
                &mut scene,
                &format_duration(audio.duration_ms()),
                Rect {
                    x: rect.x + 10.0,
                    y: rect.y + 23.0,
                    width: rect.width - 44.0,
                    height: 17.0,
                },
                HAlign::Left,
                10.0,
                MUTED,
            );
            label(
                &mut scene,
                "×",
                Rect {
                    x: rect.x + rect.width - 30.0,
                    y: rect.y,
                    width: 30.0,
                    height: rect.height,
                },
                HAlign::Center,
                18.0,
                MUTED,
            );
            scene.controls.push(SceneControl {
                id: format!("voicelines.audio.{}", audio.id),
                label: audio.file_name.clone(),
                bounds: rect,
                role: AccessibleRole::Button,
                selected,
            });
            scene.controls.push(SceneControl {
                id: format!("voicelines.audio.remove.{}", audio.id),
                label: format!("Retirer {}", audio.file_name),
                bounds: Rect {
                    x: rect.x + rect.width - 36.0,
                    y: rect.y,
                    width: 36.0,
                    height: rect.height,
                },
                role: AccessibleRole::Button,
                selected: false,
            });
        }

        let automatic_name = self
            .naming_pattern
            .clone()
            .unwrap_or_else(|| editable_automatic_name(project.automatic_pattern()));
        let naming = if self.naming_pattern.is_some() {
            format!("Nom auto : {automatic_name}|")
        } else {
            format!("Nom auto : {automatic_name}")
        };
        let button_labels = [
            "+ Ajouter",
            "Détection auto",
            &naming,
            "Sauvegarder",
            "Charger",
        ];
        for ((rect, action), text) in layout.buttons().into_iter().zip(button_labels) {
            scene.quads.push(quad(rect, PANEL_ALT, BORDER, 7.0));
            label(&mut scene, text, rect, HAlign::Center, 12.0, TEXT);
            scene.controls.push(SceneControl {
                id: format!("voicelines.header.{action:?}"),
                label: text.to_string(),
                bounds: rect,
                role: AccessibleRole::Button,
                selected: false,
            });
        }

        let Some(audio) = project.active_audio() else {
            label(
                &mut scene,
                "Glissez un ou plusieurs fichiers audio ici\nou cliquez pour importer",
                layout.content,
                HAlign::Center,
                21.0,
                MUTED,
            );
            return scene;
        };

        render_waveform(&mut scene, audio, self, current_ms, layout);
        label(
            &mut scene,
            "Zones de découpe  •  Ctrl+clic pour sélectionner  •  double-clic pour renommer  •  clic droit pour écouter  •  Maj+clic pour exporter  •  Ctrl+molette pour zoomer",
            Rect { x: layout.regions.x + 12.0, y: layout.regions.y + 6.0, width: layout.regions.width - 24.0, height: 24.0 },
            HAlign::Left,
            12.0,
            MUTED,
        );
        for (row, region) in audio
            .regions
            .iter()
            .skip(self.region_scroll)
            .take(visible_region_rows(layout))
            .enumerate()
        {
            let rect = region_row_rect(layout, row);
            let selected = self.selected_regions.contains(&region.id);
            scene.quads.push(quad(
                rect,
                if selected {
                    [0.16, 0.14, 0.30, 1.0]
                } else {
                    PANEL_ALT
                },
                if selected { ACCENT } else { [0.0; 4] },
                5.0,
            ));
            let text = self
                .rename
                .as_ref()
                .filter(|(id, _)| *id == region.id)
                .map(|(_, text)| format!("{text}│"))
                .unwrap_or_else(|| region.name.clone());
            label(
                &mut scene,
                &text,
                inset(rect, 8.0, 0.0),
                HAlign::Left,
                12.0,
                TEXT,
            );
            label(
                &mut scene,
                &format!(
                    "{}  →  {}",
                    format_duration(region.start_ms),
                    format_duration(region.end_ms)
                ),
                Rect {
                    x: rect.x + rect.width - 230.0,
                    y: rect.y,
                    width: 220.0,
                    height: rect.height,
                },
                HAlign::Right,
                11.0,
                MUTED,
            );
            scene.controls.push(SceneControl {
                id: format!("voicelines.region.{}", region.id),
                label: format!(
                    "{} : {} à {}",
                    region.name,
                    format_duration(region.start_ms),
                    format_duration(region.end_ms)
                ),
                bounds: rect,
                role: AccessibleRole::Button,
                selected,
            });
        }
        if let Some((track, thumb, _)) =
            region_scrollbar_geometry(layout, audio.regions.len(), self.region_scroll)
        {
            scene
                .quads
                .push(quad(track, [0.07, 0.07, 0.09, 1.0], [0.0; 4], 4.0));
            scene
                .quads
                .push(quad(thumb, [0.36, 0.37, 0.44, 1.0], [0.0; 4], 4.0));
        }
        if let Some(menu) = self.audio_context_menu {
            push_audio_context_menu(&mut scene, menu, self.recording_transfer_disabled);
        }
        scene
    }

    fn handle_rename(&mut self, event: &UiEvent) -> Option<EventResponse> {
        let (region_id, text) = self.rename.as_mut()?;
        match event {
            UiEvent::KeyInput { text: input } if input == "\x1b" => {
                self.rename = None;
                Some(EventResponse::Consumed)
            }
            UiEvent::KeyInput { text: input } if input == "\r" || input == "\n" => {
                let action = UiAction::VoicelinesRenameRegion {
                    region_id: *region_id,
                    name: text.clone(),
                };
                self.rename = None;
                Some(EventResponse::Action(action))
            }
            UiEvent::KeyInput { text: input } if input == "\x08" || input == "\x7f" => {
                text.pop();
                Some(EventResponse::Consumed)
            }
            UiEvent::KeyInput { text: input } => {
                text.extend(
                    input
                        .chars()
                        .filter(|character| !character.is_control())
                        .take(120 - text.chars().count().min(120)),
                );
                Some(EventResponse::Consumed)
            }
            UiEvent::MousePress { .. }
            | UiEvent::CtrlClick { .. }
            | UiEvent::ShiftMousePress { .. }
            | UiEvent::DoubleClick { .. }
            | UiEvent::ContextMenu { .. } => {
                let action = UiAction::VoicelinesRenameRegion {
                    region_id: *region_id,
                    name: text.clone(),
                };
                self.rename = None;
                Some(EventResponse::Action(action))
            }
            _ => Some(EventResponse::Consumed),
        }
    }

    fn handle_naming_pattern(&mut self, event: &UiEvent) -> Option<EventResponse> {
        let pattern = self.naming_pattern.as_mut()?;
        match event {
            UiEvent::KeyInput { text } if text == "\x1b" => {
                self.naming_pattern = None;
                Some(EventResponse::Consumed)
            }
            UiEvent::KeyInput { text } if text == "\r" || text == "\n" => {
                let action = UiAction::VoicelinesSetNamingPattern(pattern.clone());
                self.naming_pattern = None;
                Some(EventResponse::Action(action))
            }
            UiEvent::KeyInput { text } if text == "\x08" || text == "\x7f" => {
                pattern.pop();
                Some(EventResponse::Consumed)
            }
            UiEvent::KeyInput { text } => {
                pattern.extend(
                    text.chars()
                        .filter(|character| !character.is_control())
                        .take(120 - pattern.chars().count().min(120)),
                );
                Some(EventResponse::Consumed)
            }
            UiEvent::MousePress { .. }
            | UiEvent::CtrlClick { .. }
            | UiEvent::ShiftMousePress { .. }
            | UiEvent::DoubleClick { .. }
            | UiEvent::ContextMenu { .. } => {
                let action = UiAction::VoicelinesSetNamingPattern(pattern.clone());
                self.naming_pattern = None;
                Some(EventResponse::Action(action))
            }
            _ => Some(EventResponse::Consumed),
        }
    }

    pub fn control_action(&mut self, id: &str, project: &VoicelinesProject) -> Option<UiAction> {
        let action = match id {
            "voicelines.header.Import" => UiAction::VoicelinesImportAudio,
            "voicelines.header.Detect" => UiAction::VoicelinesAutoDetect,
            "voicelines.header.Naming" => {
                self.begin_naming_pattern(editable_automatic_name(project.automatic_pattern()));
                return None;
            }
            "voicelines.header.Save" => UiAction::QuickSave,
            "voicelines.header.Load" => UiAction::VoicelinesLoadSession,
            _ => {
                if let Some(id) = id
                    .strip_prefix("voicelines.audio.remove.")
                    .and_then(|id| id.parse().ok())
                {
                    return Some(UiAction::VoicelinesRemoveAudio(id));
                }
                if let Some(id) = id
                    .strip_prefix("voicelines.audio.")
                    .and_then(|id| id.parse().ok())
                {
                    return Some(UiAction::VoicelinesSelectAudio(id));
                }
                if let Some(id) = id
                    .strip_prefix("voicelines.region.")
                    .and_then(|id| id.parse().ok())
                {
                    self.selected_regions = vec![id];
                    return Some(UiAction::VoicelinesSelectRegion(Some(id)));
                }
                return None;
            }
        };
        Some(action)
    }

    fn ms_at_x(&self, layout: VoicelinesLayout, x: f32, duration_ms: u64) -> u64 {
        ms_at_x(
            layout.waveform,
            self.view_start_ms,
            self.view_duration_ms,
            x,
            duration_ms,
        )
    }
}

fn editable_automatic_name(pattern: &str) -> String {
    pattern.strip_suffix("_{num}").unwrap_or(pattern).to_owned()
}

fn render_waveform(
    scene: &mut VoicelinesScene,
    audio: &Audio,
    ui: &VoicelinesWorkspaceUi,
    current_ms: u64,
    layout: VoicelinesLayout,
) {
    let wave = layout.waveform;
    scene
        .quads
        .push(quad(wave, [0.035, 0.038, 0.05, 1.0], BORDER, 8.0));
    let center = wave.y + wave.height * 0.52;
    scene.quads.push(quad(
        Rect {
            x: wave.x + 1.0,
            y: center,
            width: wave.width - 2.0,
            height: 1.0,
        },
        [0.28, 0.29, 0.34, 0.8],
        [0.0; 4],
        0.0,
    ));
    let view_end = ui.view_start_ms.saturating_add(ui.view_duration_ms);
    let peaks = &audio.waveform.peaks;
    let peak_ms = f64::from(audio.waveform.samples_per_peak.max(1)) * 1_000.0
        / f64::from(audio.sample_rate.max(1));
    if !peaks.is_empty() && peak_ms > 0.0 {
        let first = (ui.view_start_ms as f64 / peak_ms).floor() as usize;
        let last = ((view_end as f64 / peak_ms).ceil() as usize).min(peaks.len());
        let columns = wave.width.max(1.0) as usize;
        for column in 0..columns {
            let a = first + (last.saturating_sub(first) * column / columns.max(1));
            let b = first + (last.saturating_sub(first) * (column + 1) / columns.max(1));
            let peak = peaks[a.min(peaks.len() - 1)..b.max(a + 1).min(peaks.len())]
                .iter()
                .copied()
                .fold(0.0_f32, f32::max);
            let height = (peak * (wave.height - 34.0) * 0.46).max(1.0);
            scene.quads.push(quad(
                Rect {
                    x: wave.x + column as f32,
                    y: center - height,
                    width: 1.2,
                    height: height * 2.0,
                },
                [0.38, 0.62, 0.96, 0.9],
                [0.0; 4],
                0.0,
            ));
        }
    }

    for region in &audio.regions {
        let (start_ms, end_ms) =
            preview_bounds(ui.drag, region).unwrap_or((region.start_ms, region.end_ms));
        if end_ms < ui.view_start_ms || start_ms > view_end {
            continue;
        }
        let x1 = x_at_ms(wave, ui.view_start_ms, ui.view_duration_ms, start_ms);
        let x2 = x_at_ms(wave, ui.view_start_ms, ui.view_duration_ms, end_ms);
        let color = region_color(region.id, ui.selected_regions.contains(&region.id));
        let rect = Rect {
            x: x1,
            y: wave.y + 22.0,
            width: (x2 - x1).max(2.0),
            height: wave.height - 30.0,
        };
        scene.quads.push(quad(
            rect,
            color,
            if ui.selected_regions.contains(&region.id) {
                [0.82, 0.78, 1.0, 1.0]
            } else {
                color
            },
            5.0,
        ));
        scene.quads.push(quad(
            Rect {
                x: rect.x,
                y: rect.y,
                width: HANDLE_W,
                height: rect.height,
            },
            ACCENT,
            [0.0; 4],
            3.0,
        ));
        scene.quads.push(quad(
            Rect {
                x: rect.x + rect.width - HANDLE_W,
                y: rect.y,
                width: HANDLE_W,
                height: rect.height,
            },
            ACCENT,
            [0.0; 4],
            3.0,
        ));
        label(
            scene,
            &region.name,
            Rect {
                x: rect.x + 9.0,
                y: rect.y + 4.0,
                width: (rect.width - 18.0).max(0.0),
                height: 20.0,
            },
            HAlign::Center,
            11.0,
            TEXT,
        );
    }

    if let Some(Drag::Create {
        anchor_ms,
        current_ms,
    }) = ui.drag
    {
        let x1 = x_at_ms(
            wave,
            ui.view_start_ms,
            ui.view_duration_ms,
            anchor_ms.min(current_ms),
        );
        let x2 = x_at_ms(
            wave,
            ui.view_start_ms,
            ui.view_duration_ms,
            anchor_ms.max(current_ms),
        );
        scene.quads.push(quad(
            Rect {
                x: x1,
                y: wave.y + 22.0,
                width: (x2 - x1).max(2.0),
                height: wave.height - 30.0,
            },
            [0.38, 0.31, 0.88, 0.38],
            ACCENT,
            5.0,
        ));
    }

    if current_ms >= ui.view_start_ms && current_ms <= view_end {
        let x = x_at_ms(wave, ui.view_start_ms, ui.view_duration_ms, current_ms);
        scene.quads.push(quad(
            Rect {
                x: x - 1.0,
                y: wave.y + 16.0,
                width: 2.0,
                height: wave.height - 17.0,
            },
            [0.95, 0.25, 0.30, 1.0],
            [0.0; 4],
            0.0,
        ));
    }

    for tick in 0..=5 {
        let ms = ui.view_start_ms + ui.view_duration_ms * tick / 5;
        let x = wave.x + wave.width * tick as f32 / 5.0;
        label(
            scene,
            &format_duration(ms),
            Rect {
                x: x - 35.0,
                y: wave.y + 2.0,
                width: 70.0,
                height: 17.0,
            },
            HAlign::Center,
            9.0,
            MUTED,
        );
    }
}

fn push_audio_context_menu(
    scene: &mut VoicelinesScene,
    menu: AudioContextMenu,
    recording_transfer_disabled: bool,
) {
    let parent = Rect {
        x: menu.x,
        y: menu.y,
        width: AUDIO_MENU_W,
        height: AUDIO_MENU_ITEM_H,
    };
    let has_update = menu.delivered_comic_dubs || menu.delivered_recording;
    let menu_rect = Rect {
        height: AUDIO_MENU_ITEM_H * if has_update { 2.0 } else { 1.0 },
        ..parent
    };
    scene
        .system_quads
        .push(quad(menu_rect, [0.13, 0.13, 0.16, 0.99], BORDER, 0.0));
    for (index, (mode, text)) in [
        (AudioMenuMode::Send, "Envoyer les voicelines vers"),
        (AudioMenuMode::Update, "Mettre à jour chez"),
    ]
    .into_iter()
    .take(if has_update { 2 } else { 1 })
    .enumerate()
    {
        let row = Rect {
            y: parent.y + index as f32 * AUDIO_MENU_ITEM_H,
            ..parent
        };
        if menu.hover_parent == Some(mode) {
            scene.system_quads.push(quad(
                inset(row, 3.0, 2.0),
                [0.31, 0.40, 0.72, 0.85],
                [0.0; 4],
                0.0,
            ));
        }
        system_label(
            scene,
            text,
            Rect {
                x: row.x + 10.0,
                width: row.width - 38.0,
                ..row
            },
            HAlign::Left,
            12.0,
            TEXT,
        );
        system_label(
            scene,
            ">",
            Rect {
                x: row.x + row.width - 24.0,
                width: 18.0,
                ..row
            },
            HAlign::Center,
            12.0,
            MUTED,
        );
    }

    if let Some(mode) = menu.open_mode {
        let submenu = Rect {
            x: parent.x + parent.width - 2.0,
            height: AUDIO_MENU_ITEM_H * 2.0,
            ..parent
        };
        scene
            .system_quads
            .push(quad(submenu, [0.13, 0.13, 0.16, 0.99], BORDER, 0.0));
        for (index, (text, workspace, enabled)) in [
            (
                "Comic Dubs",
                crate::application::workspace_service::WorkspaceId::ComicDubs,
                !matches!(mode, AudioMenuMode::Update) || menu.delivered_comic_dubs,
            ),
            (
                "Enregistrement",
                crate::application::workspace_service::WorkspaceId::Recording,
                !recording_transfer_disabled
                    && (!matches!(mode, AudioMenuMode::Update) || menu.delivered_recording),
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let rect = Rect {
                y: submenu.y + index as f32 * AUDIO_MENU_ITEM_H,
                height: AUDIO_MENU_ITEM_H,
                ..submenu
            };
            if enabled && menu.hover_target == Some(workspace) {
                scene.system_quads.push(quad(
                    inset(rect, 3.0, 2.0),
                    [0.31, 0.40, 0.72, 0.85],
                    [0.0; 4],
                    0.0,
                ));
            }
            system_label(
                scene,
                text,
                inset(rect, 10.0, 0.0),
                HAlign::Left,
                12.0,
                if enabled { TEXT } else { MUTED },
            );
        }
    }
}

fn finish_drag(drag: Drag, duration_ms: u64) -> EventResponse {
    let action = match drag {
        Drag::Create {
            anchor_ms,
            current_ms,
        } => {
            let (start_ms, end_ms) = ordered(anchor_ms, current_ms);
            if end_ms.saturating_sub(start_ms) < 20 {
                UiAction::SeekAbsolute((start_ms / 10) as i64)
            } else {
                UiAction::VoicelinesAddRegion { start_ms, end_ms }
            }
        }
        Drag::Start {
            region_id,
            end_ms,
            current_ms,
        } => UiAction::VoicelinesMoveRegion {
            region_id,
            start_ms: current_ms.min(end_ms.saturating_sub(20)),
            end_ms,
        },
        Drag::End {
            region_id,
            start_ms,
            current_ms,
        } => UiAction::VoicelinesMoveRegion {
            region_id,
            start_ms,
            end_ms: current_ms.max(start_ms.saturating_add(20)).min(duration_ms),
        },
        Drag::Move {
            region_id,
            anchor_pointer_ms,
            current_ms,
            start_ms,
            end_ms,
        } => {
            let duration = end_ms - start_ms;
            let offset = current_ms as i128 - anchor_pointer_ms as i128;
            let mut start = (start_ms as i128 + offset).max(0) as u64;
            if start.saturating_add(duration) > duration_ms {
                start = duration_ms.saturating_sub(duration);
            }
            UiAction::VoicelinesMoveRegion {
                region_id,
                start_ms: start,
                end_ms: start.saturating_add(duration),
            }
        }
    };
    EventResponse::Action(action)
}

#[derive(Debug, Clone, Copy)]
enum RegionPart {
    Start,
    Body,
    End,
}

fn region_hit(
    audio: &Audio,
    waveform: Rect,
    view_start_ms: u64,
    view_duration_ms: u64,
    x: f32,
) -> Option<(&Region, RegionPart)> {
    audio.regions.iter().rev().find_map(|region| {
        let start = x_at_ms(waveform, view_start_ms, view_duration_ms, region.start_ms);
        let end = x_at_ms(waveform, view_start_ms, view_duration_ms, region.end_ms);
        if (x - start).abs() <= HANDLE_W {
            Some((region, RegionPart::Start))
        } else if (x - end).abs() <= HANDLE_W {
            Some((region, RegionPart::End))
        } else if x > start && x < end {
            Some((region, RegionPart::Body))
        } else {
            None
        }
    })
}

fn preview_bounds(drag: Option<Drag>, region: &Region) -> Option<(u64, u64)> {
    match drag? {
        Drag::Start {
            region_id,
            end_ms,
            current_ms,
        } if region_id == region.id => Some((current_ms.min(end_ms.saturating_sub(20)), end_ms)),
        Drag::End {
            region_id,
            start_ms,
            current_ms,
        } if region_id == region.id => Some((start_ms, current_ms.max(start_ms + 20))),
        Drag::Move {
            region_id,
            anchor_pointer_ms,
            current_ms,
            start_ms,
            end_ms,
        } if region_id == region.id => {
            let delta = current_ms as i128 - anchor_pointer_ms as i128;
            Some((
                (start_ms as i128 + delta).max(0) as u64,
                (end_ms as i128 + delta).max(20) as u64,
            ))
        }
        _ => None,
    }
}

fn audio_hit(
    project: &VoicelinesProject,
    layout: VoicelinesLayout,
    scroll: usize,
    x: f32,
    y: f32,
) -> Option<(u64, bool)> {
    project
        .audios()
        .iter()
        .skip(scroll)
        .take(visible_audio_rows(layout))
        .enumerate()
        .find_map(|(row, audio)| {
            let rect = audio_row_rect(layout, row);
            rect.contains(x, y)
                .then_some((audio.id, x >= rect.x + rect.width - 36.0))
        })
}

fn visible_audio_rows(layout: VoicelinesLayout) -> usize {
    ((layout.sidebar.height - 44.0) / (AUDIO_ROW_H + 6.0))
        .floor()
        .max(1.0) as usize
}

fn audio_row_rect(layout: VoicelinesLayout, index: usize) -> Rect {
    Rect {
        x: layout.sidebar.x + 10.0,
        y: layout.sidebar.y + 44.0 + index as f32 * (AUDIO_ROW_H + 6.0),
        width: layout.sidebar.width - 20.0,
        height: AUDIO_ROW_H,
    }
}

fn region_row_rect(layout: VoicelinesLayout, index: usize) -> Rect {
    Rect {
        x: layout.regions.x + 10.0,
        y: layout.regions.y + 32.0 + index as f32 * (REGION_ROW_H + 3.0),
        width: layout.regions.width - 34.0,
        height: REGION_ROW_H,
    }
}

fn visible_region_rows(layout: VoicelinesLayout) -> usize {
    ((layout.regions.height - 40.0) / (REGION_ROW_H + 3.0))
        .floor()
        .max(1.0) as usize
}

fn region_scrollbar_geometry(
    layout: VoicelinesLayout,
    region_count: usize,
    scroll: usize,
) -> Option<(Rect, Rect, usize)> {
    let visible = visible_region_rows(layout);
    let max_scroll = region_count.saturating_sub(visible);
    if max_scroll == 0 {
        return None;
    }
    let track = Rect {
        x: layout.regions.x + layout.regions.width - 14.0,
        y: layout.regions.y + 32.0,
        width: 7.0,
        height: (layout.regions.height - 40.0).max(1.0),
    };
    let thumb_height =
        (track.height * visible as f32 / region_count as f32).clamp(18.0, track.height);
    let travel = (track.height - thumb_height).max(0.0);
    let thumb = Rect {
        y: track.y + travel * scroll.min(max_scroll) as f32 / max_scroll as f32,
        height: thumb_height,
        ..track
    };
    Some((track, thumb, max_scroll))
}

fn region_row_hit(
    audio: &Audio,
    layout: VoicelinesLayout,
    scroll: usize,
    x: f32,
    y: f32,
) -> Option<&Region> {
    audio
        .regions
        .iter()
        .skip(scroll)
        .take(visible_region_rows(layout))
        .enumerate()
        .find_map(|(row, region)| {
            region_row_rect(layout, row)
                .contains(x, y)
                .then_some(region)
        })
}

fn ms_at_x(
    waveform: Rect,
    view_start_ms: u64,
    view_duration_ms: u64,
    x: f32,
    duration_ms: u64,
) -> u64 {
    let ratio = ((x - waveform.x) / waveform.width.max(1.0)).clamp(0.0, 1.0) as f64;
    view_start_ms
        .saturating_add((view_duration_ms as f64 * ratio).round() as u64)
        .min(duration_ms)
}

fn x_at_ms(waveform: Rect, view_start_ms: u64, view_duration_ms: u64, ms: u64) -> f32 {
    waveform.x
        + waveform.width
            * (ms.saturating_sub(view_start_ms) as f32 / view_duration_ms.max(1) as f32)
                .clamp(0.0, 1.0)
}

fn ordered(a: u64, b: u64) -> (u64, u64) {
    (a.min(b), a.max(b))
}

fn format_duration(ms: u64) -> String {
    let minutes = ms / 60_000;
    let seconds = (ms % 60_000) / 1_000;
    let millis = ms % 1_000;
    format!("{minutes:02}:{seconds:02}.{millis:03}")
}

fn region_color(id: RegionId, selected: bool) -> [f32; 4] {
    let hue = (id.wrapping_mul(47) % 255) as f32 / 255.0;
    [
        0.22 + hue * 0.16,
        0.20 + (1.0 - hue) * 0.12,
        0.52 + hue * 0.18,
        if selected { 0.72 } else { 0.48 },
    ]
}

fn inset(rect: Rect, x: f32, y: f32) -> Rect {
    Rect {
        x: rect.x + x,
        y: rect.y + y,
        width: (rect.width - x * 2.0).max(0.0),
        height: (rect.height - y * 2.0).max(0.0),
    }
}

fn quad(rect: Rect, color: [f32; 4], border_color: [f32; 4], radius: f32) -> QuadInstance {
    QuadInstance {
        rect: [rect.x, rect.y, rect.width, rect.height],
        color,
        color_bottom: color,
        border_color,
        border_width: if border_color == [0.0; 4] { 0.0 } else { 1.0 },
        border_radius: radius,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    }
}

fn label(
    scene: &mut VoicelinesScene,
    text: &str,
    bounds: Rect,
    h_align: HAlign,
    font_size: f32,
    color: [u8; 3],
) {
    scene.labels.push(SceneLabel {
        text: text.into(),
        bounds,
        h_align,
        font_size,
        color,
    });
}

fn system_label(
    scene: &mut VoicelinesScene,
    text: &str,
    bounds: Rect,
    h_align: HAlign,
    font_size: f32,
    color: [u8; 3],
) {
    scene.system_labels.push(SceneLabel {
        text: text.into(),
        bounds,
        h_align,
        font_size,
        color,
    });
}

pub fn append_scene<'a>(
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<crate::ui::primitives::LabelInfo<'a>>,
    scene: &'a VoicelinesScene,
) {
    quads.extend_from_slice(&scene.quads);
    labels.extend(
        scene
            .labels
            .iter()
            .map(|label| crate::ui::primitives::LabelInfo {
                text: label.text.as_str(),
                bounds: label.bounds,
                h_align: label.h_align,
                v_align: VAlign::Center,
                overflow: Overflow::Ellipsis,
                padding: 0.0,
                font_size_override: Some(label.font_size),
                color_override: Some(label.color),
                font_family_override: None,
            }),
    );
}

pub fn append_system<'a>(
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<crate::ui::primitives::LabelInfo<'a>>,
    scene: &'a VoicelinesScene,
) {
    quads.extend_from_slice(&scene.system_quads);
    labels.extend(
        scene
            .system_labels
            .iter()
            .map(|label| crate::ui::primitives::LabelInfo {
                text: label.text.as_str(),
                bounds: label.bounds,
                h_align: label.h_align,
                v_align: VAlign::Center,
                overflow: Overflow::Ellipsis,
                padding: 0.0,
                font_size_override: Some(label.font_size),
                color_override: Some(label.color),
                font_family_override: None,
            }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recording::{RecordedAudio, WaveformData};

    #[test]
    fn drag_creation_orders_both_directions() {
        assert!(matches!(
            finish_drag(
                Drag::Create {
                    anchor_ms: 900,
                    current_ms: 300
                },
                1_000
            ),
            EventResponse::Action(UiAction::VoicelinesAddRegion {
                start_ms: 300,
                end_ms: 900
            })
        ));
    }

    #[test]
    fn zoom_anchor_is_mapped_inside_visible_time() {
        let wave = Rect {
            x: 100.0,
            y: 0.0,
            width: 1_000.0,
            height: 100.0,
        };
        assert_eq!(ms_at_x(wave, 2_000, 10_000, 600.0, 20_000), 7_000);
    }

    fn project_with_two_regions() -> (VoicelinesProject, RegionId, RegionId) {
        let mut project = VoicelinesProject::default();
        project.add_audio(
            "voice.wav".into(),
            "voice.flac".into(),
            RecordedAudio {
                file_name: "voice.flac".into(),
                sample_rate: 48_000,
                channels: 1,
                sample_count: 48_000,
                checksum: "a".repeat(40),
                waveform: WaveformData::default(),
            },
        );
        let first = project.add_region(100, 200).unwrap();
        let second = project.add_region(300, 400).unwrap();
        (project, first, second)
    }

    #[test]
    fn ctrl_click_exposes_join_action_in_selection_order() {
        let (project, first, second) = project_with_two_regions();
        let layout = VoicelinesLayout::compute(Rect {
            x: 0.0,
            y: 0.0,
            width: 1_200.0,
            height: 700.0,
        });
        let mut ui = VoicelinesWorkspaceUi::default();
        for row in 0..2 {
            let rect = region_row_rect(layout, row);
            assert_eq!(
                ui.handle_event(
                    &UiEvent::CtrlClick {
                        x: rect.x + 4.0,
                        y: rect.y + 4.0,
                    },
                    &project,
                    layout,
                ),
                EventResponse::Consumed
            );
        }
        assert_eq!(ui.selected_regions(), &[first, second]);
    }

    #[test]
    fn clicking_away_commits_region_rename() {
        let (project, first, _) = project_with_two_regions();
        let layout = VoicelinesLayout::compute(Rect {
            x: 0.0,
            y: 0.0,
            width: 1_200.0,
            height: 700.0,
        });
        let mut ui = VoicelinesWorkspaceUi::default();
        ui.sync(&project, layout);
        ui.begin_rename(first, "nouveau nom".into());
        assert_eq!(
            ui.handle_event(
                &UiEvent::MousePress {
                    x: layout.header.x + 2.0,
                    y: layout.header.y + 2.0,
                },
                &project,
                layout,
            ),
            EventResponse::Action(UiAction::VoicelinesRenameRegion {
                region_id: first,
                name: "nouveau nom".into(),
            })
        );
    }

    #[test]
    fn empty_workspace_keeps_session_loading_available() {
        let layout = VoicelinesLayout::compute(Rect {
            x: 0.0,
            y: 0.0,
            width: 1_200.0,
            height: 700.0,
        });
        let load = layout.buttons()[4].0;
        let mut ui = VoicelinesWorkspaceUi::default();
        assert_eq!(
            ui.handle_event(
                &UiEvent::MousePress {
                    x: load.x + 1.0,
                    y: load.y + 1.0,
                },
                &VoicelinesProject::default(),
                layout,
            ),
            EventResponse::Action(UiAction::VoicelinesLoadSession)
        );
    }

    #[test]
    fn save_button_saves_the_whole_project() {
        let layout = VoicelinesLayout::compute(Rect {
            x: 0.0,
            y: 0.0,
            width: 1_200.0,
            height: 700.0,
        });
        let save = layout.buttons()[3].0;
        let mut ui = VoicelinesWorkspaceUi::default();
        assert_eq!(
            ui.handle_event(
                &UiEvent::MousePress {
                    x: save.x + 1.0,
                    y: save.y + 1.0,
                },
                &VoicelinesProject::default(),
                layout,
            ),
            EventResponse::Action(UiAction::QuickSave)
        );
    }

    #[test]
    fn automatic_naming_is_one_non_overlapping_control() {
        let layout = VoicelinesLayout::compute(Rect {
            x: 0.0,
            y: 0.0,
            width: 1_200.0,
            height: 700.0,
        });
        let project = VoicelinesProject::default();
        let mut ui = VoicelinesWorkspaceUi::default();
        let naming = layout.buttons()[2].0;
        let scene = ui.scene(&project, 0, layout);

        for pair in layout.buttons().windows(2) {
            assert!(pair[0].0.x + pair[0].0.width <= pair[1].0.x);
        }
        assert!(naming.y + naming.height <= layout.toolbar.y);
        assert!(scene
            .labels
            .iter()
            .any(|label| label.text == "Nom auto : voiceline"));

        assert_eq!(
            ui.handle_event(
                &UiEvent::MousePress {
                    x: naming.x + 1.0,
                    y: naming.y + 1.0,
                },
                &project,
                layout,
            ),
            EventResponse::Consumed
        );
        assert_eq!(ui.naming_pattern.as_deref(), Some("voiceline"));
        for _ in 0..9 {
            ui.handle_event(
                &UiEvent::KeyInput {
                    text: "\x08".into(),
                },
                &project,
                layout,
            );
        }
        ui.handle_event(
            &UiEvent::KeyInput {
                text: "dialogue".into(),
            },
            &project,
            layout,
        );
        assert_eq!(
            ui.handle_event(&UiEvent::KeyInput { text: "\r".into() }, &project, layout,),
            EventResponse::Action(UiAction::VoicelinesSetNamingPattern("dialogue".into()))
        );
    }

    #[test]
    fn audio_context_menu_sends_voicelines_to_both_destinations() {
        let layout = VoicelinesLayout::compute(Rect {
            x: 0.0,
            y: 0.0,
            width: 1_200.0,
            height: 700.0,
        });
        let mut project = VoicelinesProject::default();
        let audio_id = project.add_audio(
            "voice.wav".into(),
            "voice.flac".into(),
            RecordedAudio {
                file_name: "voice.flac".into(),
                sample_rate: 48_000,
                channels: 1,
                sample_count: 48_000,
                checksum: "a".repeat(40),
                waveform: WaveformData::default(),
            },
        );
        let row = audio_row_rect(layout, 0);

        for (index, workspace) in [
            crate::application::workspace_service::WorkspaceId::ComicDubs,
            crate::application::workspace_service::WorkspaceId::Recording,
        ]
        .into_iter()
        .enumerate()
        {
            let mut ui = VoicelinesWorkspaceUi::default();
            assert_eq!(
                ui.handle_event(
                    &UiEvent::ContextMenu {
                        x: row.x + 10.0,
                        y: row.y + 10.0,
                    },
                    &project,
                    layout,
                ),
                EventResponse::Consumed
            );
            let menu = ui.audio_context_menu.unwrap();
            let rendered = ui.scene(&project, 0, layout);
            assert!(!rendered
                .labels
                .iter()
                .any(|label| label.text == "Envoyer les voicelines vers"));
            assert!(rendered
                .system_labels
                .iter()
                .any(|label| label.text == "Envoyer les voicelines vers"));
            ui.handle_event(
                &UiEvent::MousePress {
                    x: menu.x + 10.0,
                    y: menu.y + 10.0,
                },
                &project,
                layout,
            );
            assert_eq!(
                ui.handle_event(
                    &UiEvent::MousePress {
                        x: menu.x + AUDIO_MENU_W + 10.0,
                        y: menu.y + AUDIO_MENU_ITEM_H * index as f32 + 10.0,
                    },
                    &project,
                    layout,
                ),
                EventResponse::Action(UiAction::VoicelinesSendAudio {
                    audio_id,
                    workspace,
                })
            );
        }
    }

    #[test]
    fn audio_context_menu_updates_an_existing_delivery() {
        let layout = VoicelinesLayout::compute(Rect {
            x: 0.0,
            y: 0.0,
            width: 1_200.0,
            height: 700.0,
        });
        let mut project = VoicelinesProject::default();
        let audio_id = project.add_audio(
            "voice.wav".into(),
            "voice.flac".into(),
            RecordedAudio {
                file_name: "voice.flac".into(),
                sample_rate: 48_000,
                channels: 1,
                sample_count: 48_000,
                checksum: "a".repeat(40),
                waveform: WaveformData::default(),
            },
        );
        let region_id = project.add_region(0, 500).unwrap();
        project.set_delivery_target(
            audio_id,
            crate::voicelines::DeliveryDestination::ComicDubs,
            region_id,
            42,
        );
        let row = audio_row_rect(layout, 0);
        let mut ui = VoicelinesWorkspaceUi::default();
        ui.handle_event(
            &UiEvent::ContextMenu {
                x: row.x + 10.0,
                y: row.y + 10.0,
            },
            &project,
            layout,
        );
        let menu = ui.audio_context_menu.unwrap();
        assert!(ui
            .scene(&project, 0, layout)
            .system_labels
            .iter()
            .any(|label| label.text == "Mettre à jour chez"));
        ui.handle_event(
            &UiEvent::MousePress {
                x: menu.x + 10.0,
                y: menu.y + AUDIO_MENU_ITEM_H + 10.0,
            },
            &project,
            layout,
        );
        assert_eq!(
            ui.handle_event(
                &UiEvent::MousePress {
                    x: menu.x + AUDIO_MENU_W + 10.0,
                    y: menu.y + 10.0,
                },
                &project,
                layout,
            ),
            EventResponse::Action(UiAction::VoicelinesUpdateAudio {
                audio_id,
                workspace: crate::application::workspace_service::WorkspaceId::ComicDubs,
            })
        );
    }

    #[test]
    fn server_session_disables_sending_voicelines_to_recording() {
        let layout = VoicelinesLayout::compute(Rect {
            x: 0.0,
            y: 0.0,
            width: 1_200.0,
            height: 700.0,
        });
        let mut project = VoicelinesProject::default();
        project.add_audio(
            "voice.wav".into(),
            "voice.flac".into(),
            RecordedAudio {
                file_name: "voice.flac".into(),
                sample_rate: 48_000,
                channels: 1,
                sample_count: 48_000,
                checksum: "a".repeat(40),
                waveform: WaveformData::default(),
            },
        );
        let row = audio_row_rect(layout, 0);
        let mut ui = VoicelinesWorkspaceUi::default();
        ui.set_recording_transfer_disabled(true);
        ui.handle_event(
            &UiEvent::ContextMenu {
                x: row.x + 10.0,
                y: row.y + 10.0,
            },
            &project,
            layout,
        );
        let menu = ui.audio_context_menu.unwrap();
        ui.handle_event(
            &UiEvent::MousePress {
                x: menu.x + 10.0,
                y: menu.y + 10.0,
            },
            &project,
            layout,
        );
        assert_eq!(
            ui.scene(&project, 0, layout)
                .system_labels
                .iter()
                .find(|label| label.text == "Enregistrement")
                .unwrap()
                .color,
            MUTED
        );
        assert_eq!(
            ui.handle_event(
                &UiEvent::MousePress {
                    x: menu.x + AUDIO_MENU_W + 10.0,
                    y: menu.y + AUDIO_MENU_ITEM_H + 10.0,
                },
                &project,
                layout,
            ),
            EventResponse::Consumed
        );
    }

    #[test]
    fn region_scrollbar_reaches_the_last_cut_zone() {
        let layout = VoicelinesLayout::compute(Rect {
            x: 0.0,
            y: 0.0,
            width: 1_200.0,
            height: 700.0,
        });
        let (_, first, max_scroll) = region_scrollbar_geometry(layout, 12, 0).unwrap();
        let (_, last, _) = region_scrollbar_geometry(layout, 12, max_scroll).unwrap();

        let mut project = VoicelinesProject::default();
        project.add_audio(
            "voice.wav".into(),
            "voice.flac".into(),
            RecordedAudio {
                file_name: "voice.flac".into(),
                sample_rate: 1_000,
                channels: 1,
                sample_count: 20_000,
                checksum: "a".repeat(40),
                waveform: WaveformData::default(),
            },
        );
        let selected = project.add_region(0, 100).unwrap();
        for index in 1..12 {
            project.add_region(index * 200, index * 200 + 100);
        }
        let mut ui = VoicelinesWorkspaceUi::default();
        ui.set_selected_region(Some(selected));
        for _ in 0..12 {
            ui.handle_event(
                &UiEvent::Scroll {
                    x: layout.regions.x + 2.0,
                    y: layout.regions.y + 50.0,
                    delta: -1.0,
                    ctrl: false,
                    fast: false,
                },
                &project,
                layout,
            );
        }

        assert_eq!(max_scroll, 9);
        assert_eq!(ui.region_scroll, max_scroll);
        assert!(last.y > first.y);
        assert!(last.y + last.height <= layout.regions.y + layout.regions.height - 8.0);
    }
}
