//! Backend-neutral Comic Dubs layout, interaction and scene generation.

use crate::comic_dubs::{Bubble, BubbleId, ComicAudioId, ComicDubsProject, Page, PageId, Point};
use crate::ui::color_picker::ColorPickerState;
use crate::ui::focus::AccessibleRole;
use crate::ui::primitives::{
    EventResponse, HAlign, IconInstance, LabelInfo, Overflow, QuadInstance, Rect, UiAction,
    UiEvent, VAlign,
};

const SIDEBAR_W: f32 = 292.0;
const INSPECTOR_W: f32 = 260.0;
const HEADER_H: f32 = 52.0;
const TOOLBAR_H: f32 = 42.0;
const ROW_H: f32 = 44.0;
const BG: [f32; 4] = [0.052, 0.055, 0.07, 1.0];
const PANEL: [f32; 4] = [0.082, 0.087, 0.108, 1.0];
const PANEL_ALT: [f32; 4] = [0.115, 0.12, 0.15, 1.0];
const BORDER: [f32; 4] = [0.24, 0.25, 0.31, 0.9];
const ACCENT: [f32; 4] = [0.38, 0.31, 0.88, 1.0];
const TEXT: [u8; 3] = [232, 234, 242];
const MUTED: [u8; 3] = [151, 155, 172];
#[derive(Debug, Clone, Copy, Default)]
pub struct ComicDubsLayout {
    pub content: Rect,
    pub sidebar: Rect,
    pub inspector: Rect,
    pub header: Rect,
    pub toolbar: Rect,
    pub canvas: Rect,
}

impl ComicDubsLayout {
    pub fn compute(content: Rect) -> Self {
        let sidebar_w = SIDEBAR_W.min((content.width * 0.34).max(210.0));
        let sidebar = Rect {
            x: content.x,
            y: content.y,
            width: sidebar_w,
            height: content.height,
        };
        let inspector_w = INSPECTOR_W.min((content.width * 0.3).max(220.0));
        let inspector = Rect {
            x: content.x + content.width - inspector_w,
            y: content.y,
            width: inspector_w,
            height: content.height,
        };
        let main = Rect {
            x: sidebar.x + sidebar.width,
            y: content.y,
            width: (content.width - sidebar.width - inspector.width).max(0.0),
            height: content.height,
        };
        let header = Rect {
            x: main.x,
            y: main.y,
            width: main.width,
            height: HEADER_H,
        };
        let toolbar = Rect {
            y: header.y + header.height,
            height: TOOLBAR_H,
            ..header
        };
        let canvas = Rect {
            x: main.x + 12.0,
            y: toolbar.y + toolbar.height + 12.0,
            width: (main.width - 24.0).max(0.0),
            height: (main.height - header.height - toolbar.height - 24.0).max(0.0),
        };
        Self {
            content,
            sidebar,
            inspector,
            header,
            toolbar,
            canvas,
        }
    }

    fn image_tab(self) -> Rect {
        Rect {
            x: self.sidebar.x + 10.0,
            y: self.sidebar.y + 10.0,
            width: (self.sidebar.width - 24.0) * 0.5,
            height: 32.0,
        }
    }

    fn audio_tab(self) -> Rect {
        let image = self.image_tab();
        Rect {
            x: image.x + image.width + 4.0,
            ..image
        }
    }

    fn previous(self) -> Rect {
        header_button(self.header, 10.0, 56.0)
    }

    fn next(self) -> Rect {
        header_button(self.header, 70.0, 56.0)
    }
}

fn header_button(header: Rect, offset: f32, width: f32) -> Rect {
    Rect {
        x: header.x + offset,
        y: header.y + 10.0,
        width,
        height: 32.0,
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum MediaTab {
    #[default]
    Images,
    Audios,
}

#[derive(Debug, Clone)]
pub struct SceneLabel {
    pub text: String,
    pub bounds: Rect,
    pub h_align: HAlign,
    pub font_size: f32,
    pub color: [u8; 3],
    pub font_family: Option<String>,
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
pub struct ComicDubsScene {
    pub quads: Vec<QuadInstance>,
    pub overlay_quads: Vec<QuadInstance>,
    pub labels: Vec<SceneLabel>,
    pub overlay_labels: Vec<SceneLabel>,
    pub controls: Vec<SceneControl>,
    pub page_rect: Option<Rect>,
    pub page_id: Option<PageId>,
}

#[derive(Debug, Clone)]
struct BubbleDrag {
    bubble_id: BubbleId,
    anchor: Point,
    original: Vec<Point>,
    delta: Point,
}

#[derive(Debug, Clone)]
struct BubbleVertexDrag {
    bubble_id: BubbleId,
    index: usize,
    original: Vec<Point>,
    points: Vec<Point>,
}

#[derive(Debug, Clone, Copy)]
struct DraftVertexDrag {
    index: usize,
    original: Point,
    moved: bool,
}

#[derive(Default)]
pub struct ComicDubsWorkspaceUi {
    media_tab: MediaTab,
    media_scroll: usize,
    selected_bubble: Option<BubbleId>,
    draft: Vec<Point>,
    text_edit: Option<(BubbleId, String)>,
    dragging_audio: Option<ComicAudioId>,
    drag_position: (f32, f32),
    bubble_drag: Option<BubbleDrag>,
    bubble_vertex_drag: Option<BubbleVertexDrag>,
    draft_vertex_drag: Option<DraftVertexDrag>,
    playback: Option<(PageId, usize)>,
    color_bubble: Option<BubbleId>,
    color_picker: ColorPickerState,
}

impl ComicDubsWorkspaceUi {
    pub fn ensure_color_picker_textures(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bind_group_layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
    ) {
        self.color_picker
            .ensure_textures(device, queue, bind_group_layout, sampler);
    }

    pub fn render_color_picker<'a>(
        &'a self,
        bg: &mut Vec<QuadInstance>,
        textures: &mut Vec<(IconInstance, &'a wgpu::BindGroup)>,
        fg: &mut Vec<QuadInstance>,
    ) {
        self.color_picker.render(bg, textures, fg);
    }

    pub fn is_editing_text(&self) -> bool {
        self.text_edit.is_some()
    }

    pub fn selected_bubble(&self) -> Option<BubbleId> {
        self.selected_bubble
    }

    pub fn set_playback(&mut self, page_id: Option<PageId>, visible_bubbles: usize) {
        self.playback = page_id.map(|page_id| (page_id, visible_bubbles));
        if page_id.is_some() {
            self.text_edit = None;
            self.selected_bubble = None;
        }
    }

    pub fn drop_accepts(&self, layout: ComicDubsLayout, x: f32, y: f32) -> bool {
        layout.sidebar.contains(x, y)
    }

    pub fn begin_text_edit(&mut self, bubble_id: BubbleId, text: String) {
        self.selected_bubble = Some(bubble_id);
        self.text_edit = Some((bubble_id, text));
    }

    pub fn cancel_draft(&mut self) -> bool {
        let cancelled = !self.draft.is_empty();
        self.draft.clear();
        self.draft_vertex_drag = None;
        cancelled
    }

    pub fn control_action(&mut self, id: &str, project: &ComicDubsProject) -> Option<UiAction> {
        if let Some(id) = id
            .strip_prefix("comic.page.")
            .and_then(|id| id.parse().ok())
        {
            return Some(UiAction::ComicDubsSelectPage(id));
        }
        if let Some(id) = id
            .strip_prefix("comic.bubble.")
            .or_else(|| id.strip_prefix("comic.canvas.bubble."))
            .and_then(|id| id.parse().ok())
        {
            project.bubble(id)?;
            self.selected_bubble = Some(id);
            return None;
        }
        if let Some(audio_id) = id
            .strip_prefix("comic.audio.")
            .and_then(|id| id.parse().ok())
        {
            if let Some(bubble_id) = self.selected_bubble {
                return Some(UiAction::ComicDubsAssignAudio {
                    bubble_id,
                    audio_id: Some(audio_id),
                });
            }
        }
        None
    }

    pub fn clear_document_state(&mut self) {
        *self = Self::default();
    }

    pub fn sync(&mut self, project: &ComicDubsProject, layout: ComicDubsLayout) {
        if self
            .selected_bubble
            .is_some_and(|id| project.bubble(id).is_none())
        {
            self.selected_bubble = None;
            self.text_edit = None;
        }
        let count = match self.media_tab {
            MediaTab::Images => project.pages().len(),
            MediaTab::Audios => project.audios().len(),
        };
        self.media_scroll = self
            .media_scroll
            .min(count.saturating_sub(visible_media_rows(layout)));
    }

    pub fn handle_event(
        &mut self,
        event: &UiEvent,
        project: &ComicDubsProject,
        layout: ComicDubsLayout,
    ) -> EventResponse {
        self.sync(project, layout);
        if self.color_picker.active {
            let before = self.color_picker.current_color();
            if self.color_picker.handle_event(event) {
                let after = self.color_picker.current_color();
                let bubble_id = self.color_bubble;
                if !self.color_picker.active {
                    self.color_bubble = None;
                }
                return bubble_id.filter(|_| before != after).map_or(
                    EventResponse::Consumed,
                    |bubble_id| {
                        EventResponse::Action(UiAction::ComicDubsSetBubbleColor {
                            bubble_id,
                            color: rgba8(after),
                        })
                    },
                );
            }
        }
        if let Some(response) = self.handle_text_edit(event) {
            return response;
        }

        let page = project.active_page();
        let page_rect = page.map(|page| image_rect(layout.canvas, page));

        if let (UiEvent::ContextMenu { x, y }, Some(page), Some(rect)) = (event, page, page_rect) {
            let Some(bubble_id) = bubble_at(page, rect, *x, *y) else {
                return EventResponse::Ignored;
            };
            self.selected_bubble = Some(bubble_id);
            return project
                .bubble(bubble_id)
                .and_then(|bubble| bubble.audio_id)
                .map_or(EventResponse::Consumed, |audio_id| {
                    EventResponse::Action(UiAction::ComicDubsAssignAudio {
                        bubble_id,
                        audio_id: Some(audio_id),
                    })
                });
        }

        if matches!(event, UiEvent::KeyInput { text } if text == "\x1b") && !self.draft.is_empty() {
            self.cancel_draft();
            return EventResponse::Consumed;
        }

        if let (UiEvent::CtrlClick { x, y }, Some(_page), Some(rect)) = (event, page, page_rect) {
            if rect.contains(*x, *y) {
                self.draft.clear();
                self.draft.push(point_at(rect, *x, *y));
                self.selected_bubble = None;
                return EventResponse::Consumed;
            }
        }

        if let (UiEvent::MousePress { x, y }, Some(_page), Some(rect)) = (event, page, page_rect) {
            if !self.draft.is_empty() && rect.contains(*x, *y) {
                let point = point_at(rect, *x, *y);
                if let Some(index) = vertex_at(rect, &self.draft, *x, *y) {
                    self.draft_vertex_drag = Some(DraftVertexDrag {
                        index,
                        original: self.draft[index],
                        moved: false,
                    });
                    return EventResponse::Consumed;
                }
                if self.draft.len() < 128 {
                    self.draft.push(point);
                }
                return EventResponse::Consumed;
            }
        }

        if let UiEvent::MousePress { x, y } = event {
            if layout.image_tab().contains(*x, *y) {
                self.media_tab = MediaTab::Images;
                self.media_scroll = 0;
                return EventResponse::Consumed;
            }
            if layout.audio_tab().contains(*x, *y) {
                self.media_tab = MediaTab::Audios;
                self.media_scroll = 0;
                return EventResponse::Consumed;
            }
        }

        if let Some(response) = self.handle_header(event, project, layout) {
            return response;
        }
        if let Some(response) = self.handle_inspector(event, project, layout) {
            return response;
        }
        if let Some(response) = self.handle_media(event, project, layout) {
            return response;
        }

        if let UiEvent::MouseMove { x, y } = event {
            if self.dragging_audio.is_some() {
                self.drag_position = (*x, *y);
                return EventResponse::Consumed;
            }
            if let (Some(drag), Some(rect)) = (self.draft_vertex_drag.as_mut(), page_rect) {
                let point = point_at(rect, *x, *y);
                drag.moved |= (point.x - drag.original.x).abs() > 0.001
                    || (point.y - drag.original.y).abs() > 0.001;
                self.draft[drag.index] = point;
                return EventResponse::Consumed;
            }
            if let (Some(drag), Some(rect)) = (self.bubble_vertex_drag.as_mut(), page_rect) {
                drag.points[drag.index] = point_at(rect, *x, *y);
                return EventResponse::Consumed;
            }
            if let (Some(drag), Some(rect)) = (self.bubble_drag.as_mut(), page_rect) {
                let pointer = point_at(rect, *x, *y);
                let min_x = drag
                    .original
                    .iter()
                    .map(|point| point.x)
                    .fold(1.0, f32::min);
                let max_x = drag
                    .original
                    .iter()
                    .map(|point| point.x)
                    .fold(0.0, f32::max);
                let min_y = drag
                    .original
                    .iter()
                    .map(|point| point.y)
                    .fold(1.0, f32::min);
                let max_y = drag
                    .original
                    .iter()
                    .map(|point| point.y)
                    .fold(0.0, f32::max);
                drag.delta = Point {
                    x: (pointer.x - drag.anchor.x).clamp(-min_x, 1.0 - max_x),
                    y: (pointer.y - drag.anchor.y).clamp(-min_y, 1.0 - max_y),
                };
                return EventResponse::Consumed;
            }
        }
        if let UiEvent::MouseRelease { x, y } = event {
            if let Some(audio_id) = self.dragging_audio.take() {
                let bubble_id = page
                    .zip(page_rect)
                    .and_then(|(page, rect)| bubble_at(page, rect, *x, *y));
                return bubble_id.map_or(EventResponse::Consumed, |bubble_id| {
                    EventResponse::Action(UiAction::ComicDubsAssignAudio {
                        bubble_id,
                        audio_id: Some(audio_id),
                    })
                });
            }
            if let Some(drag) = self.draft_vertex_drag.take() {
                if drag.index == 0 && !drag.moved && self.draft.len() >= 3 {
                    let points = std::mem::take(&mut self.draft);
                    return EventResponse::Action(UiAction::ComicDubsAddBubble {
                        page_id: page.unwrap().id,
                        points,
                    });
                }
                return EventResponse::Consumed;
            }
            if let Some(drag) = self.bubble_vertex_drag.take() {
                return if drag.points == drag.original {
                    EventResponse::Consumed
                } else {
                    EventResponse::Action(UiAction::ComicDubsSetBubblePoints {
                        bubble_id: drag.bubble_id,
                        points: drag.points,
                    })
                };
            }
            if let Some(drag) = self.bubble_drag.take() {
                let points = translated_drag_points(&drag);
                return if points == drag.original {
                    EventResponse::Consumed
                } else {
                    EventResponse::Action(UiAction::ComicDubsSetBubblePoints {
                        bubble_id: drag.bubble_id,
                        points,
                    })
                };
            }
        }

        if let (UiEvent::DoubleClick { x, y }, Some(page), Some(rect)) = (event, page, page_rect) {
            if let Some(id) = bubble_at(page, rect, *x, *y) {
                self.bubble_drag = None;
                let text = project.bubble(id).unwrap().text.clone();
                self.begin_text_edit(id, text);
                return EventResponse::Consumed;
            }
        }
        if let (UiEvent::MousePress { x, y }, Some(page), Some(rect)) = (event, page, page_rect) {
            if rect.contains(*x, *y) {
                if let Some(bubble_id) = self.selected_bubble {
                    let bubble = project.bubble(bubble_id).unwrap();
                    if let Some(index) = vertex_at(rect, &bubble.points, *x, *y) {
                        self.bubble_vertex_drag = Some(BubbleVertexDrag {
                            bubble_id,
                            index,
                            original: bubble.points.clone(),
                            points: bubble.points.clone(),
                        });
                        return EventResponse::Consumed;
                    }
                }
                let hit = bubble_at(page, rect, *x, *y);
                self.selected_bubble = hit;
                self.bubble_drag = self.selected_bubble.and_then(|bubble_id| {
                    project.bubble(bubble_id).map(|bubble| BubbleDrag {
                        bubble_id,
                        anchor: point_at(rect, *x, *y),
                        original: bubble.points.clone(),
                        delta: Point { x: 0.0, y: 0.0 },
                    })
                });
                return EventResponse::Consumed;
            }
        }
        if matches!(event, UiEvent::Delete) {
            if let Some(id) = self.selected_bubble.take() {
                return EventResponse::Action(UiAction::ComicDubsRemoveBubble(id));
            }
        }
        if let UiEvent::Scroll { x, y, delta, .. } = event {
            if layout.sidebar.contains(*x, *y) {
                let count = match self.media_tab {
                    MediaTab::Images => project.pages().len(),
                    MediaTab::Audios => project.audios().len(),
                };
                self.media_scroll = scroll_rows(
                    self.media_scroll,
                    *delta,
                    count.saturating_sub(visible_media_rows(layout)),
                );
                return EventResponse::Consumed;
            }
        }
        if matches!(event, UiEvent::MousePress { .. }) {
            self.selected_bubble = None;
            self.text_edit = None;
            self.bubble_drag = None;
        }
        EventResponse::Ignored
    }

    fn handle_header(
        &mut self,
        event: &UiEvent,
        project: &ComicDubsProject,
        layout: ComicDubsLayout,
    ) -> Option<EventResponse> {
        let UiEvent::MousePress { x, y } = event else {
            return None;
        };
        let active = project
            .active_page_id()
            .and_then(|id| project.pages().iter().position(|page| page.id == id));
        if layout.previous().contains(*x, *y) {
            let page = active
                .and_then(|index| index.checked_sub(1))
                .and_then(|index| project.pages().get(index));
            return Some(page.map_or(EventResponse::Consumed, |page| {
                EventResponse::Action(UiAction::ComicDubsSelectPage(page.id))
            }));
        }
        if layout.next().contains(*x, *y) {
            let page = active.and_then(|index| project.pages().get(index + 1));
            return Some(page.map_or(EventResponse::Consumed, |page| {
                EventResponse::Action(UiAction::ComicDubsSelectPage(page.id))
            }));
        }
        None
    }

    fn handle_inspector(
        &mut self,
        event: &UiEvent,
        project: &ComicDubsProject,
        layout: ComicDubsLayout,
    ) -> Option<EventResponse> {
        let UiEvent::MousePress { x, y } = event else {
            return None;
        };
        if !layout.inspector.contains(*x, *y) {
            return None;
        }
        let Some(bubble_id) = self.selected_bubble else {
            return Some(EventResponse::Consumed);
        };
        let bubble = project.bubble(bubble_id)?;
        if inspector_minus(layout, 88.0).contains(*x, *y) {
            return Some(EventResponse::Action(
                UiAction::ComicDubsSetBubbleFontSize {
                    bubble_id,
                    font_size: bubble.font_size - 2.0,
                },
            ));
        }
        if inspector_plus(layout, 88.0).contains(*x, *y) {
            return Some(EventResponse::Action(
                UiAction::ComicDubsSetBubbleFontSize {
                    bubble_id,
                    font_size: bubble.font_size + 2.0,
                },
            ));
        }
        let swatch = inspector_color(layout);
        if swatch.contains(*x, *y) {
            let (picker_w, picker_h) = ColorPickerState::panel_size();
            let picker_x = (layout.inspector.x - picker_w - 8.0).max(4.0);
            let picker_y = swatch
                .y
                .min(layout.content.y + layout.content.height - picker_h - 4.0)
                .max(layout.content.y + 4.0);
            self.color_picker
                .open(picker_x, picker_y, rgba(bubble.color));
            self.color_bubble = Some(bubble_id);
            return Some(EventResponse::Consumed);
        }
        if inspector_action(layout, 230.0, 0).contains(*x, *y) {
            return Some(EventResponse::Action(UiAction::ComicDubsMoveBubble {
                bubble_id,
                delta: -1,
            }));
        }
        if inspector_action(layout, 230.0, 1).contains(*x, *y) {
            return Some(EventResponse::Action(UiAction::ComicDubsMoveBubble {
                bubble_id,
                delta: 1,
            }));
        }
        let delete = Rect {
            x: layout.inspector.x + 12.0,
            y: layout.inspector.y + 278.0,
            width: layout.inspector.width - 24.0,
            height: 34.0,
        };
        if delete.contains(*x, *y) {
            self.selected_bubble = None;
            return Some(EventResponse::Action(UiAction::ComicDubsRemoveBubble(
                bubble_id,
            )));
        }
        Some(EventResponse::Consumed)
    }

    fn handle_media(
        &mut self,
        event: &UiEvent,
        project: &ComicDubsProject,
        layout: ComicDubsLayout,
    ) -> Option<EventResponse> {
        let UiEvent::MousePress { x, y } = event else {
            return None;
        };
        let body_y = layout.sidebar.y + 52.0;
        if *y < body_y || !layout.sidebar.contains(*x, *y) {
            return None;
        }
        let row = ((*y - body_y) / ROW_H).floor().max(0.0) as usize + self.media_scroll;
        let local_x = *x - layout.sidebar.x;
        match self.media_tab {
            MediaTab::Images => {
                let page = project.pages().get(row)?;
                if local_x >= layout.sidebar.width - 34.0 {
                    return Some(EventResponse::Action(UiAction::ComicDubsRemovePage(
                        page.id,
                    )));
                }
                if local_x >= layout.sidebar.width - 66.0 {
                    return Some(EventResponse::Action(UiAction::ComicDubsMovePage {
                        page_id: page.id,
                        delta: 1,
                    }));
                }
                if local_x >= layout.sidebar.width - 98.0 {
                    return Some(EventResponse::Action(UiAction::ComicDubsMovePage {
                        page_id: page.id,
                        delta: -1,
                    }));
                }
                Some(EventResponse::Action(UiAction::ComicDubsSelectPage(
                    page.id,
                )))
            }
            MediaTab::Audios => {
                let audio = project.audios().get(row)?;
                if local_x >= layout.sidebar.width - 34.0 {
                    return Some(EventResponse::Action(UiAction::ComicDubsRemoveAudio(
                        audio.id,
                    )));
                }
                self.dragging_audio = Some(audio.id);
                self.drag_position = (*x, *y);
                Some(EventResponse::Consumed)
            }
        }
    }

    fn handle_text_edit(&mut self, event: &UiEvent) -> Option<EventResponse> {
        if matches!(
            event,
            UiEvent::MousePress { .. } | UiEvent::DoubleClick { .. }
        ) {
            self.text_edit = None;
            return None;
        }
        let (id, text) = self.text_edit.as_mut()?;
        match event {
            UiEvent::KeyInput { text: input } if input == "\x1b" => {
                self.text_edit = None;
                Some(EventResponse::Consumed)
            }
            UiEvent::KeyInput { text: input } if input == "\r" || input == "\n" => {
                self.text_edit = None;
                Some(EventResponse::Consumed)
            }
            UiEvent::KeyInput { text: input } if input == "\x08" || input == "\x7f" => {
                text.pop();
                Some(EventResponse::Action(UiAction::ComicDubsSetBubbleText {
                    bubble_id: *id,
                    text: text.clone(),
                }))
            }
            UiEvent::KeyInput { text: input } => {
                text.extend(
                    input
                        .chars()
                        .filter(|character| !character.is_control())
                        .take(500 - text.chars().count().min(500)),
                );
                Some(EventResponse::Action(UiAction::ComicDubsSetBubbleText {
                    bubble_id: *id,
                    text: text.clone(),
                }))
            }
            _ => Some(EventResponse::Consumed),
        }
    }

    pub fn scene(&self, project: &ComicDubsProject, layout: ComicDubsLayout) -> ComicDubsScene {
        let mut scene = ComicDubsScene::default();
        scene.quads.push(quad(layout.content, BG, [0.0; 4], 0.0));
        scene.quads.push(quad(layout.sidebar, PANEL, BORDER, 0.0));
        scene.quads.push(quad(layout.inspector, PANEL, BORDER, 0.0));
        scene.quads.push(quad(layout.header, PANEL, BORDER, 0.0));
        self.render_media(project, layout, &mut scene);
        self.render_header(project, layout, &mut scene);
        self.render_inspector(project, layout, &mut scene);

        let Some(page) = project.active_page() else {
            label(
                &mut scene,
                "Importez ou déposez des images pour commencer votre Comic Dub",
                layout.canvas,
                HAlign::Center,
                20.0,
                MUTED,
            );
            return scene;
        };
        let rect = image_rect(layout.canvas, page);
        scene.page_rect = Some(rect);
        scene.page_id = Some(page.id);
        scene
            .quads
            .push(quad(rect, [0.04, 0.04, 0.05, 1.0], BORDER, 2.0));
        for (index, bubble) in page.bubbles.iter().enumerate() {
            let playback_state = self
                .playback
                .filter(|(page_id, _)| *page_id == page.id)
                .map(|(_, visible)| {
                    crate::comic_dubs::bubble_playback_state(bubble, index, visible)
                });
            if playback_state.is_some_and(|(show_background, _)| !show_background) {
                continue;
            }
            let points = self
                .bubble_vertex_drag
                .as_ref()
                .filter(|drag| drag.bubble_id == bubble.id)
                .map(|drag| drag.points.clone())
                .or_else(|| {
                    self.bubble_drag
                        .as_ref()
                        .filter(|drag| drag.bubble_id == bubble.id)
                        .map(translated_drag_points)
                });
            let bubble_points = points.as_deref().unwrap_or(&bubble.points);
            render_bubble(
                &mut scene,
                bubble,
                bubble_points,
                rect,
                self.selected_bubble == Some(bubble.id),
                self.text_edit.as_ref(),
                playback_state.is_none_or(|(_, show_text)| show_text),
                project.font_family(),
            );
            if self.playback.is_none() {
                render_reading_order_badge(
                    &mut scene,
                    rect,
                    bubble_points,
                    index + 1,
                    bubble.audio_id.is_some(),
                );
            }
        }
        if !self.draft.is_empty() {
            for edge in self.draft.windows(2) {
                scene
                    .overlay_quads
                    .push(line_quad(rect, edge[0], edge[1], ACCENT, 2.0));
            }
            if self.draft.len() >= 3 {
                scene.overlay_quads.push(line_quad(
                    rect,
                    *self.draft.last().unwrap(),
                    self.draft[0],
                    [ACCENT[0], ACCENT[1], ACCENT[2], 0.45],
                    1.0,
                ));
            }
            for (index, point) in self.draft.iter().enumerate() {
                let center = screen_point(rect, *point);
                scene.overlay_quads.push(quad(
                    Rect {
                        x: center.0 - 5.0,
                        y: center.1 - 5.0,
                        width: 10.0,
                        height: 10.0,
                    },
                    if index == 0 {
                        [0.3, 0.9, 0.6, 1.0]
                    } else {
                        ACCENT
                    },
                    [1.0; 4],
                    5.0,
                ));
            }
            label(
                &mut scene,
                "Cliquez pour ajouter un sommet • cliquez le premier point pour fermer • Échap pour annuler",
                Rect {
                    x: rect.x,
                    y: rect.y - 28.0,
                    width: rect.width,
                    height: 24.0,
                },
                HAlign::Center,
                12.0,
                TEXT,
            );
        }
        if let Some(audio_id) = self.dragging_audio {
            let name = project
                .audio(audio_id)
                .map(|audio| audio.file_name.as_str())
                .unwrap_or("Audio");
            let drag = Rect {
                x: self.drag_position.0 + 10.0,
                y: self.drag_position.1 + 10.0,
                width: 190.0,
                height: 30.0,
            };
            scene
                .overlay_quads
                .push(quad(drag, [0.15, 0.13, 0.28, 0.96], ACCENT, 6.0));
            overlay_label(&mut scene, name, drag, HAlign::Center, 12.0, TEXT);
        }
        scene
    }

    fn render_media(
        &self,
        project: &ComicDubsProject,
        layout: ComicDubsLayout,
        scene: &mut ComicDubsScene,
    ) {
        for (rect, text, active) in [
            (
                layout.image_tab(),
                "Images",
                self.media_tab == MediaTab::Images,
            ),
            (
                layout.audio_tab(),
                "Audios",
                self.media_tab == MediaTab::Audios,
            ),
        ] {
            scene.quads.push(quad(
                rect,
                if active { ACCENT } else { PANEL_ALT },
                BORDER,
                6.0,
            ));
            label(scene, text, rect, HAlign::Center, 13.0, TEXT);
        }
        let body_y = layout.sidebar.y + 52.0;
        match self.media_tab {
            MediaTab::Images => {
                for (row, page) in project
                    .pages()
                    .iter()
                    .skip(self.media_scroll)
                    .take(visible_media_rows(layout))
                    .enumerate()
                {
                    let rect = media_row(layout, row);
                    let selected = project.active_page_id() == Some(page.id);
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
                    label(
                        scene,
                        &format!("{}  {}×{}", page.file_name, page.width, page.height),
                        Rect {
                            width: rect.width - 102.0,
                            ..rect
                        },
                        HAlign::Left,
                        12.0,
                        TEXT,
                    );
                    label(
                        scene,
                        "↑  ↓  ×",
                        Rect {
                            x: rect.x + rect.width - 96.0,
                            width: 92.0,
                            ..rect
                        },
                        HAlign::Center,
                        15.0,
                        MUTED,
                    );
                    scene.controls.push(SceneControl {
                        id: format!("comic.page.{}", page.id),
                        label: page.file_name.clone(),
                        bounds: rect,
                        role: AccessibleRole::Button,
                        selected,
                    });
                }
            }
            MediaTab::Audios => {
                for (row, audio) in project
                    .audios()
                    .iter()
                    .skip(self.media_scroll)
                    .take(visible_media_rows(layout))
                    .enumerate()
                {
                    let rect = media_row(layout, row);
                    scene.quads.push(quad(rect, PANEL_ALT, [0.0; 4], 5.0));
                    label(
                        scene,
                        &audio.file_name,
                        Rect {
                            width: rect.width - 42.0,
                            ..rect
                        },
                        HAlign::Left,
                        12.0,
                        TEXT,
                    );
                    label(
                        scene,
                        &format!("{:.1} s", audio.duration_ms() as f64 / 1_000.0),
                        Rect {
                            y: rect.y + 22.0,
                            height: 18.0,
                            width: rect.width - 42.0,
                            ..rect
                        },
                        HAlign::Left,
                        10.0,
                        MUTED,
                    );
                    label(
                        scene,
                        "×",
                        Rect {
                            x: rect.x + rect.width - 34.0,
                            width: 30.0,
                            ..rect
                        },
                        HAlign::Center,
                        18.0,
                        MUTED,
                    );
                    scene.controls.push(SceneControl {
                        id: format!("comic.audio.{}", audio.id),
                        label: format!("{}; glisser sur une bulle", audio.file_name),
                        bounds: rect,
                        role: AccessibleRole::Button,
                        selected: false,
                    });
                }
            }
        }
        let hint = match self.media_tab {
            MediaTab::Images => "Déposez des images ici • conversion PNG automatique",
            MediaTab::Audios => {
                "Déposez des audios ici • conversion FLAC automatique • glissez-les sur une bulle"
            }
        };
        label(
            scene,
            hint,
            Rect {
                x: layout.sidebar.x + 10.0,
                y: layout.sidebar.y + layout.sidebar.height - 48.0,
                width: layout.sidebar.width - 20.0,
                height: 40.0,
            },
            HAlign::Center,
            10.0,
            MUTED,
        );
        let _ = body_y;
    }

    fn render_header(
        &self,
        project: &ComicDubsProject,
        layout: ComicDubsLayout,
        scene: &mut ComicDubsScene,
    ) {
        let buttons = [(layout.previous(), "←"), (layout.next(), "→")];
        for (rect, text) in buttons {
            scene.quads.push(quad(rect, PANEL_ALT, BORDER, 6.0));
            label(scene, text, rect, HAlign::Center, 16.0, TEXT);
        }
        let active = project
            .active_page_id()
            .and_then(|id| project.pages().iter().position(|page| page.id == id))
            .map(|index| index + 1)
            .unwrap_or(0);
        label(
            scene,
            &format!("Page {active}/{}", project.pages().len()),
            Rect {
                x: layout.header.x + 140.0,
                width: 150.0,
                ..layout.previous()
            },
            HAlign::Left,
            13.0,
            TEXT,
        );
        label(
            scene,
            if self.playback.is_some() {
                "Lecture en cours • Espace pour arrêter"
            } else {
                "Espace pour lire le Comic Dub"
            },
            Rect {
                x: layout.header.x + 300.0,
                width: (layout.header.width - 312.0).max(0.0),
                ..layout.previous()
            },
            HAlign::Right,
            11.0,
            MUTED,
        );
    }

    fn render_inspector(
        &self,
        project: &ComicDubsProject,
        layout: ComicDubsLayout,
        scene: &mut ComicDubsScene,
    ) {
        label(
            scene,
            "INSPECTEUR",
            Rect {
                x: layout.inspector.x + 12.0,
                y: layout.inspector.y + 10.0,
                width: layout.inspector.width - 24.0,
                height: 28.0,
            },
            HAlign::Left,
            12.0,
            MUTED,
        );
        label(
            scene,
            "BULLE SÉLECTIONNÉE",
            Rect {
                x: layout.inspector.x + 12.0,
                y: layout.inspector.y + 48.0,
                width: layout.inspector.width - 24.0,
                height: 24.0,
            },
            HAlign::Left,
            11.0,
            MUTED,
        );
        let Some(bubble) = self.selected_bubble.and_then(|id| project.bubble(id)) else {
            label(
                scene,
                "Cliquez une bulle pour modifier ses propriétés",
                Rect {
                    x: layout.inspector.x + 12.0,
                    y: layout.inspector.y + 78.0,
                    width: layout.inspector.width - 24.0,
                    height: 54.0,
                },
                HAlign::Left,
                11.0,
                MUTED,
            );
            return;
        };
        inspector_value(
            scene,
            layout,
            88.0,
            "Taille du texte",
            &format!("{} px", bubble.font_size.round()),
        );
        label(
            scene,
            "Fond de la bulle",
            Rect {
                x: layout.inspector.x + 12.0,
                y: layout.inspector.y + 142.0,
                width: layout.inspector.width - 24.0,
                height: 20.0,
            },
            HAlign::Left,
            11.0,
            TEXT,
        );
        let color = inspector_color(layout);
        scene
            .quads
            .push(quad(color, rgba(bubble.color), BORDER, 5.0));
        let audio = bubble
            .audio_id
            .and_then(|id| project.audio(id))
            .map(|audio| format!("Audio : {}", audio.file_name))
            .unwrap_or_else(|| "Audio : aucun".into());
        label(
            scene,
            &audio,
            Rect {
                x: layout.inspector.x + 12.0,
                y: layout.inspector.y + 190.0,
                width: layout.inspector.width - 24.0,
                height: 28.0,
            },
            HAlign::Left,
            10.0,
            MUTED,
        );
        for (rect, text) in [
            (inspector_action(layout, 230.0, 0), "Ordre ↑"),
            (inspector_action(layout, 230.0, 1), "Ordre ↓"),
        ] {
            scene.quads.push(quad(rect, PANEL_ALT, BORDER, 6.0));
            label(scene, text, rect, HAlign::Center, 11.0, TEXT);
        }
        let delete = Rect {
            x: layout.inspector.x + 12.0,
            y: layout.inspector.y + 278.0,
            width: layout.inspector.width - 24.0,
            height: 34.0,
        };
        scene
            .quads
            .push(quad(delete, [0.38, 0.10, 0.14, 1.0], BORDER, 6.0));
        label(
            scene,
            "Supprimer la bulle",
            delete,
            HAlign::Center,
            11.0,
            TEXT,
        );
    }
}

fn render_bubble(
    scene: &mut ComicDubsScene,
    bubble: &Bubble,
    points: &[Point],
    page_rect: Rect,
    selected: bool,
    text_edit: Option<&(BubbleId, String)>,
    show_text: bool,
    font_family: Option<&str>,
) {
    scene.overlay_quads.extend(polygon_fill_quads(
        page_rect,
        points,
        opaque_rgba(bubble.color),
    ));
    let border = if selected {
        ACCENT
    } else {
        [0.88, 0.88, 0.92, 0.72]
    };
    for (a, b) in points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
    {
        scene.overlay_quads.push(line_quad(
            page_rect,
            *a,
            *b,
            border,
            if selected { 2.5 } else { 1.0 },
        ));
    }
    if selected {
        for point in points {
            let center = screen_point(page_rect, *point);
            scene.overlay_quads.push(quad(
                Rect {
                    x: center.0 - 6.0,
                    y: center.1 - 6.0,
                    width: 12.0,
                    height: 12.0,
                },
                ACCENT,
                [1.0; 4],
                6.0,
            ));
        }
    }
    let bounds = polygon_text_bounds(page_rect, points);
    let color = if luminance(rgba(bubble.color)) > 0.55 {
        [24, 24, 30]
    } else {
        TEXT
    };
    if show_text {
        let text = text_edit
            .filter(|(id, _)| *id == bubble.id)
            .map(|(_, text)| format!("{text}|"))
            .unwrap_or_else(|| bubble.text.clone());
        let (lines, font) = fit_text(&text, bounds, bubble.font_size);
        let line_h = font * 1.18;
        let total_h = line_h * lines.len() as f32;
        for (index, line) in lines.iter().enumerate() {
            overlay_label_with_font(
                scene,
                line,
                Rect {
                    x: bounds.x + 5.0,
                    y: bounds.y + (bounds.height - total_h) * 0.5 + index as f32 * line_h,
                    width: (bounds.width - 10.0).max(1.0),
                    height: line_h,
                },
                HAlign::Center,
                font,
                color,
                font_family,
            );
        }
    }
    scene.controls.push(SceneControl {
        id: format!("comic.canvas.bubble.{}", bubble.id),
        label: bubble.text.clone(),
        bounds,
        role: AccessibleRole::Button,
        selected,
    });
}

fn render_reading_order_badge(
    scene: &mut ComicDubsScene,
    page_rect: Rect,
    points: &[Point],
    order: usize,
    has_audio: bool,
) {
    let bubble = polygon_bounds(page_rect, points);
    let badge_width = if has_audio { 50.0 } else { 28.0 };
    let min_x = page_rect.x + 2.0;
    let max_x = (page_rect.x + page_rect.width - badge_width - 2.0).max(min_x);
    let min_y = page_rect.y + 2.0;
    let max_y = (page_rect.y + page_rect.height - 26.0).max(min_y);
    let badge = Rect {
        x: (bubble.x + bubble.width - badge_width).clamp(min_x, max_x),
        y: (bubble.y + 4.0).clamp(min_y, max_y),
        width: badge_width,
        height: 24.0,
    };
    scene.overlay_quads.push(quad(
        badge,
        [0.08, 0.08, 0.11, 0.92],
        [0.9, 0.9, 0.96, 0.9],
        8.0,
    ));
    overlay_label(
        scene,
        &order.to_string(),
        Rect {
            width: 26.0,
            ..badge
        },
        HAlign::Center,
        12.0,
        TEXT,
    );
    if has_audio {
        for rect in [
            Rect {
                x: badge.x + 28.0,
                y: badge.y + 8.0,
                width: 4.0,
                height: 8.0,
            },
            Rect {
                x: badge.x + 32.0,
                y: badge.y + 6.0,
                width: 5.0,
                height: 12.0,
            },
            Rect {
                x: badge.x + 40.0,
                y: badge.y + 7.0,
                width: 2.0,
                height: 10.0,
            },
            Rect {
                x: badge.x + 45.0,
                y: badge.y + 5.0,
                width: 2.0,
                height: 14.0,
            },
        ] {
            scene
                .overlay_quads
                .push(quad(rect, [0.84, 0.88, 1.0, 1.0], [0.0; 4], 1.0));
        }
    }
}

fn fit_text(text: &str, bounds: Rect, preferred_font_size: f32) -> (Vec<String>, f32) {
    let maximum = preferred_font_size.clamp(6.0, 72.0).floor() as u32;
    for font in (6..=maximum).rev().map(|size| size as f32) {
        let max_chars = (bounds.width / (font * 0.56)).floor().max(1.0) as usize;
        let lines = wrap_text(text, max_chars);
        if lines.len() as f32 * font * 1.18 <= bounds.height {
            return (lines, font);
        }
    }

    let font = 6.0;
    let max_chars = (bounds.width / (font * 0.56)).floor().max(1.0) as usize;
    let max_lines = (bounds.height / (font * 1.18)).floor().max(1.0) as usize;
    let mut lines = wrap_text(text, max_chars);
    if lines.len() > max_lines {
        lines.truncate(max_lines);
        if let Some(last) = lines.last_mut() {
            while last.chars().count() >= max_chars {
                last.pop();
            }
            last.push('…');
        }
    }
    (lines, font)
}

fn wrap_text(text: &str, max_chars: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let characters: Vec<_> = word.chars().collect();
        if characters.len() > max_chars {
            if !current.is_empty() {
                lines.push(std::mem::take(&mut current));
            }
            for chunk in characters.chunks(max_chars) {
                let chunk: String = chunk.iter().collect();
                if chunk.chars().count() == max_chars {
                    lines.push(chunk);
                } else {
                    current = chunk;
                }
            }
            continue;
        }
        if !current.is_empty() && current.chars().count() + 1 + characters.len() > max_chars {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn image_rect(canvas: Rect, page: &Page) -> Rect {
    let scale = (canvas.width / page.width as f32)
        .min(canvas.height / page.height as f32)
        .max(0.0);
    let width = page.width as f32 * scale;
    let height = page.height as f32 * scale;
    Rect {
        x: canvas.x + (canvas.width - width) * 0.5,
        y: canvas.y + (canvas.height - height) * 0.5,
        width,
        height,
    }
}

fn point_at(rect: Rect, x: f32, y: f32) -> Point {
    Point {
        x: ((x - rect.x) / rect.width.max(1.0)).clamp(0.0, 1.0),
        y: ((y - rect.y) / rect.height.max(1.0)).clamp(0.0, 1.0),
    }
}

fn screen_point(rect: Rect, point: Point) -> (f32, f32) {
    (
        rect.x + point.x * rect.width,
        rect.y + point.y * rect.height,
    )
}

fn near_vertex(rect: Rect, point: Point, x: f32, y: f32) -> bool {
    let point = screen_point(rect, point);
    (point.0 - x).hypot(point.1 - y) <= 12.0
}

fn vertex_at(rect: Rect, points: &[Point], x: f32, y: f32) -> Option<usize> {
    points
        .iter()
        .position(|point| near_vertex(rect, *point, x, y))
}

fn bubble_at(page: &Page, rect: Rect, x: f32, y: f32) -> Option<BubbleId> {
    if !rect.contains(x, y) {
        return None;
    }
    let point = point_at(rect, x, y);
    page.bubbles
        .iter()
        .rev()
        .find(|bubble| point_in_polygon(point, &bubble.points))
        .map(|bubble| bubble.id)
}

pub(crate) fn point_in_polygon(point: Point, polygon: &[Point]) -> bool {
    let mut inside = false;
    let mut previous = polygon.last().copied().unwrap_or(point);
    for current in polygon {
        if (current.y > point.y) != (previous.y > point.y)
            && point.x
                < (previous.x - current.x) * (point.y - current.y) / (previous.y - current.y)
                    + current.x
        {
            inside = !inside;
        }
        previous = *current;
    }
    inside
}

fn polygon_bounds(rect: Rect, points: &[Point]) -> Rect {
    let min_x = points.iter().map(|point| point.x).fold(1.0, f32::min);
    let max_x = points.iter().map(|point| point.x).fold(0.0, f32::max);
    let min_y = points.iter().map(|point| point.y).fold(1.0, f32::min);
    let max_y = points.iter().map(|point| point.y).fold(0.0, f32::max);
    Rect {
        x: rect.x + min_x * rect.width,
        y: rect.y + min_y * rect.height,
        width: (max_x - min_x) * rect.width,
        height: (max_y - min_y) * rect.height,
    }
}

fn polygon_text_bounds(rect: Rect, points: &[Point]) -> Rect {
    let min_x = points.iter().map(|point| point.x).fold(1.0, f32::min);
    let max_x = points.iter().map(|point| point.x).fold(0.0, f32::max);
    let min_y = points.iter().map(|point| point.y).fold(1.0, f32::min);
    let max_y = points.iter().map(|point| point.y).fold(0.0, f32::max);
    let mut best = None;
    // ponytail: a six-cell search is fast and visually stable; raise the grid only if narrow,
    // highly concave bubbles become a real editing case.
    const GRID: usize = 6;
    for left in 0..GRID {
        for right in left + 1..=GRID {
            for top in 0..GRID {
                for bottom in top + 1..=GRID {
                    let x1 = min_x + (max_x - min_x) * left as f32 / GRID as f32;
                    let x2 = min_x + (max_x - min_x) * right as f32 / GRID as f32;
                    let y1 = min_y + (max_y - min_y) * top as f32 / GRID as f32;
                    let y2 = min_y + (max_y - min_y) * bottom as f32 / GRID as f32;
                    let inset_x = (x2 - x1) * 0.06;
                    let inset_y = (y2 - y1) * 0.06;
                    let candidate = (x1 + inset_x, y1 + inset_y, x2 - inset_x, y2 - inset_y);
                    let inside = [
                        Point {
                            x: candidate.0,
                            y: candidate.1,
                        },
                        Point {
                            x: candidate.2,
                            y: candidate.1,
                        },
                        Point {
                            x: candidate.0,
                            y: candidate.3,
                        },
                        Point {
                            x: candidate.2,
                            y: candidate.3,
                        },
                        Point {
                            x: (candidate.0 + candidate.2) * 0.5,
                            y: (candidate.1 + candidate.3) * 0.5,
                        },
                    ]
                    .into_iter()
                    .all(|point| point_in_polygon(point, points));
                    let area = (candidate.2 - candidate.0) * (candidate.3 - candidate.1);
                    if inside && best.is_none_or(|(_, best_area)| area > best_area) {
                        best = Some((candidate, area));
                    }
                }
            }
        }
    }
    let Some(((x1, y1, x2, y2), _)) = best else {
        return polygon_bounds(rect, points);
    };
    Rect {
        x: rect.x + x1 * rect.width,
        y: rect.y + y1 * rect.height,
        width: (x2 - x1) * rect.width,
        height: (y2 - y1) * rect.height,
    }
}

fn translated_drag_points(drag: &BubbleDrag) -> Vec<Point> {
    drag.original
        .iter()
        .map(|point| Point {
            x: point.x + drag.delta.x,
            y: point.y + drag.delta.y,
        })
        .collect()
}

fn polygon_fill_quads(rect: Rect, points: &[Point], color: [f32; 4]) -> Vec<QuadInstance> {
    let points = points
        .iter()
        .map(|point| screen_point(rect, *point))
        .collect::<Vec<_>>();
    let min_y = points.iter().map(|point| point.1).fold(f32::MAX, f32::min);
    let max_y = points.iter().map(|point| point.1).fold(f32::MIN, f32::max);
    let mut quads = Vec::new();
    // ponytail: 3 px scanlines keep concave fills dependency-free; triangulate only if
    // very large pages or hundreds of simultaneous bubbles become a measured bottleneck.
    let step = 3.0;
    let mut y = min_y;
    while y < max_y {
        let sample_y = (y + step * 0.5).min(max_y);
        let mut intersections = Vec::new();
        for (a, b) in points
            .iter()
            .zip(points.iter().cycle().skip(1))
            .take(points.len())
        {
            if (a.1 > sample_y) != (b.1 > sample_y) {
                intersections.push(a.0 + (sample_y - a.1) * (b.0 - a.0) / (b.1 - a.1));
            }
        }
        intersections.sort_by(f32::total_cmp);
        for span in intersections.chunks_exact(2) {
            quads.push(quad(
                Rect {
                    x: span[0],
                    y,
                    width: (span[1] - span[0]).max(0.0),
                    height: (max_y - y).min(step + 0.5),
                },
                color,
                [0.0; 4],
                0.0,
            ));
        }
        y += step;
    }
    quads
}

fn line_quad(rect: Rect, a: Point, b: Point, color: [f32; 4], thickness: f32) -> QuadInstance {
    let a = screen_point(rect, a);
    let b = screen_point(rect, b);
    let length = (b.0 - a.0).hypot(b.1 - a.1);
    QuadInstance {
        rect: [
            (a.0 + b.0 - length) * 0.5,
            (a.1 + b.1 - thickness) * 0.5,
            length,
            thickness,
        ],
        color,
        color_bottom: color,
        border_color: [0.0; 4],
        border_width: 0.0,
        border_radius: thickness * 0.5,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: (b.1 - a.1).atan2(b.0 - a.0),
        _padding: [0.0; 2],
    }
}

fn inspector_row(layout: ComicDubsLayout, y: f32) -> Rect {
    Rect {
        x: layout.inspector.x + 12.0,
        y: layout.inspector.y + y,
        width: layout.inspector.width - 24.0,
        height: 40.0,
    }
}

fn inspector_minus(layout: ComicDubsLayout, y: f32) -> Rect {
    let row = inspector_row(layout, y);
    Rect { width: 34.0, ..row }
}

fn inspector_plus(layout: ComicDubsLayout, y: f32) -> Rect {
    let row = inspector_row(layout, y);
    Rect {
        x: row.x + row.width - 34.0,
        width: 34.0,
        ..row
    }
}

fn inspector_color(layout: ComicDubsLayout) -> Rect {
    Rect {
        x: layout.inspector.x + layout.inspector.width - 54.0,
        y: layout.inspector.y + 138.0,
        width: 42.0,
        height: 28.0,
    }
}

fn inspector_action(layout: ComicDubsLayout, y: f32, column: usize) -> Rect {
    let gap = 8.0;
    let width = (layout.inspector.width - 24.0 - gap) * 0.5;
    Rect {
        x: layout.inspector.x + 12.0 + column as f32 * (width + gap),
        y: layout.inspector.y + y,
        width,
        height: 34.0,
    }
}

fn inspector_value(
    scene: &mut ComicDubsScene,
    layout: ComicDubsLayout,
    y: f32,
    name: &str,
    value: &str,
) {
    let row = inspector_row(layout, y);
    for (rect, text) in [
        (inspector_minus(layout, y), "−"),
        (inspector_plus(layout, y), "+"),
    ] {
        scene.quads.push(quad(rect, PANEL_ALT, BORDER, 5.0));
        label(scene, text, rect, HAlign::Center, 15.0, TEXT);
    }
    let center = Rect {
        x: row.x + 38.0,
        width: row.width - 76.0,
        height: row.height * 0.5,
        ..row
    };
    label(scene, name, center, HAlign::Center, 10.0, MUTED);
    label(
        scene,
        value,
        Rect {
            y: center.y + center.height,
            ..center
        },
        HAlign::Center,
        11.0,
        TEXT,
    );
}

fn media_row(layout: ComicDubsLayout, row: usize) -> Rect {
    Rect {
        x: layout.sidebar.x + 8.0,
        y: layout.sidebar.y + 52.0 + row as f32 * ROW_H,
        width: layout.sidebar.width - 16.0,
        height: ROW_H - 4.0,
    }
}

fn visible_media_rows(layout: ComicDubsLayout) -> usize {
    ((layout.sidebar.height - 104.0) / ROW_H).floor().max(1.0) as usize
}

fn scroll_rows(current: usize, delta: f32, max: usize) -> usize {
    if delta > 0.0 {
        current.saturating_sub(1)
    } else if delta < 0.0 {
        current.saturating_add(1).min(max)
    } else {
        current
    }
}

fn quad(rect: Rect, color: [f32; 4], border: [f32; 4], radius: f32) -> QuadInstance {
    QuadInstance {
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
    }
}

fn label(
    scene: &mut ComicDubsScene,
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
        font_family: None,
    });
}

fn overlay_label_with_font(
    scene: &mut ComicDubsScene,
    text: &str,
    bounds: Rect,
    h_align: HAlign,
    font_size: f32,
    color: [u8; 3],
    font_family: Option<&str>,
) {
    scene.overlay_labels.push(SceneLabel {
        text: text.into(),
        bounds,
        h_align,
        font_size,
        color,
        font_family: font_family.map(str::to_owned),
    });
}

fn overlay_label(
    scene: &mut ComicDubsScene,
    text: &str,
    bounds: Rect,
    h_align: HAlign,
    font_size: f32,
    color: [u8; 3],
) {
    scene.overlay_labels.push(SceneLabel {
        text: text.into(),
        bounds,
        h_align,
        font_size,
        color,
        font_family: None,
    });
}

fn rgba(color: [u8; 4]) -> [f32; 4] {
    color.map(|channel| channel as f32 / 255.0)
}

fn rgba8(color: [f32; 4]) -> [u8; 4] {
    [
        (color[0] * 255.0).round() as u8,
        (color[1] * 255.0).round() as u8,
        (color[2] * 255.0).round() as u8,
        255,
    ]
}

fn opaque_rgba(color: [u8; 4]) -> [f32; 4] {
    [
        color[0] as f32 / 255.0,
        color[1] as f32 / 255.0,
        color[2] as f32 / 255.0,
        1.0,
    ]
}

fn luminance(color: [f32; 4]) -> f32 {
    color[0] * 0.2126 + color[1] * 0.7152 + color[2] * 0.0722
}

pub fn append_scene<'a>(
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    scene: &'a ComicDubsScene,
) {
    quads.extend(scene.quads.iter().copied());
    labels.extend(scene.labels.iter().map(|label| LabelInfo {
        text: &label.text,
        bounds: label.bounds,
        h_align: label.h_align,
        v_align: VAlign::Center,
        overflow: Overflow::Clip,
        padding: 6.0,
        font_size_override: Some(label.font_size),
        color_override: Some(label.color),
        font_family_override: label.font_family.as_deref(),
    }));
}

pub fn append_overlay<'a>(
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    scene: &'a ComicDubsScene,
) {
    quads.extend(scene.overlay_quads.iter().copied());
    labels.extend(scene.overlay_labels.iter().map(|label| LabelInfo {
        text: &label.text,
        bounds: label.bounds,
        h_align: label.h_align,
        v_align: VAlign::Center,
        overflow: Overflow::Clip,
        padding: 6.0,
        font_size_override: Some(label.font_size),
        color_override: Some(label.color),
        font_family_override: label.font_family.as_deref(),
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recording::{RecordedAudio, WaveformData};

    fn project() -> ComicDubsProject {
        let mut project = ComicDubsProject::default();
        project.add_page("page.jpg".into(), "page.png".into(), 1_000, 1_000);
        project
    }

    #[test]
    fn transport_matches_voicelines_and_leaves_the_canvas_below_it() {
        let layout = ComicDubsLayout::compute(Rect {
            x: 0.0,
            y: 0.0,
            width: 1_200.0,
            height: 800.0,
        });
        assert_eq!(layout.toolbar.y, layout.header.y + layout.header.height);
        assert_eq!(layout.toolbar.height, 42.0);
        assert!(layout.canvas.y >= layout.toolbar.y + layout.toolbar.height);
    }

    #[test]
    fn ctrl_click_then_clicks_close_a_polygon_on_the_first_vertex() {
        let project = project();
        let layout = ComicDubsLayout::compute(Rect {
            x: 0.0,
            y: 0.0,
            width: 1_200.0,
            height: 800.0,
        });
        let page_rect = image_rect(layout.canvas, project.active_page().unwrap());
        let mut ui = ComicDubsWorkspaceUi::default();
        assert_eq!(
            ui.handle_event(
                &UiEvent::CtrlClick {
                    x: page_rect.x + 100.0,
                    y: page_rect.y + 100.0
                },
                &project,
                layout
            ),
            EventResponse::Consumed
        );
        for (x, y) in [
            (page_rect.x + 300.0, page_rect.y + 100.0),
            (page_rect.x + 200.0, page_rect.y + 300.0),
        ] {
            assert_eq!(
                ui.handle_event(&UiEvent::MousePress { x, y }, &project, layout),
                EventResponse::Consumed
            );
        }
        assert_eq!(
            ui.handle_event(
                &UiEvent::MousePress {
                    x: page_rect.x + 101.0,
                    y: page_rect.y + 101.0
                },
                &project,
                layout
            ),
            EventResponse::Consumed
        );
        assert!(matches!(
            ui.handle_event(
                &UiEvent::MouseRelease {
                    x: page_rect.x + 101.0,
                    y: page_rect.y + 101.0
                },
                &project,
                layout
            ),
            EventResponse::Action(UiAction::ComicDubsAddBubble { .. })
        ));
    }

    #[test]
    fn a_draft_vertex_can_be_moved_before_closing() {
        let project = project();
        let layout = ComicDubsLayout::compute(Rect {
            x: 0.0,
            y: 0.0,
            width: 1_200.0,
            height: 800.0,
        });
        let page = image_rect(layout.canvas, project.active_page().unwrap());
        let mut ui = ComicDubsWorkspaceUi::default();
        for event in [
            UiEvent::CtrlClick {
                x: page.x + page.width * 0.2,
                y: page.y + page.height * 0.2,
            },
            UiEvent::MousePress {
                x: page.x + page.width * 0.6,
                y: page.y + page.height * 0.2,
            },
            UiEvent::MousePress {
                x: page.x + page.width * 0.4,
                y: page.y + page.height * 0.6,
            },
        ] {
            ui.handle_event(&event, &project, layout);
        }
        let second = screen_point(page, ui.draft[1]);
        ui.handle_event(
            &UiEvent::MousePress {
                x: second.0,
                y: second.1,
            },
            &project,
            layout,
        );
        ui.handle_event(
            &UiEvent::MouseMove {
                x: second.0 + page.width * 0.1,
                y: second.1 + page.height * 0.1,
            },
            &project,
            layout,
        );
        ui.handle_event(
            &UiEvent::MouseRelease {
                x: second.0 + page.width * 0.1,
                y: second.1 + page.height * 0.1,
            },
            &project,
            layout,
        );
        assert!(ui.draft[1].x > 0.69 && ui.draft[1].y > 0.29);
        assert!(ui.cancel_draft());
        assert!(ui.draft.is_empty());
    }

    #[test]
    fn polygon_hit_test_rejects_its_bounding_box_corners() {
        let triangle = [
            Point { x: 0.5, y: 0.1 },
            Point { x: 0.9, y: 0.9 },
            Point { x: 0.1, y: 0.9 },
        ];
        assert!(point_in_polygon(Point { x: 0.5, y: 0.5 }, &triangle));
        assert!(!point_in_polygon(Point { x: 0.1, y: 0.1 }, &triangle));
    }

    #[test]
    fn page_image_is_not_covered_by_an_opaque_overlay() {
        let project = project();
        let scene = ComicDubsWorkspaceUi::default().scene(
            &project,
            ComicDubsLayout::compute(Rect {
                x: 0.0,
                y: 0.0,
                width: 1_200.0,
                height: 800.0,
            }),
        );
        let page = scene.page_rect.unwrap();
        assert!(!scene.overlay_quads.iter().any(|quad| {
            quad.rect == [page.x, page.y, page.width, page.height] && quad.color[3] == 1.0
        }));
    }

    #[test]
    fn bubble_fill_and_edges_share_the_polygon_geometry() {
        let rect = Rect {
            x: 100.0,
            y: 50.0,
            width: 400.0,
            height: 300.0,
        };
        let points = [
            Point { x: 0.2, y: 0.2 },
            Point { x: 0.8, y: 0.4 },
            Point { x: 0.4, y: 0.8 },
        ];
        let fill = polygon_fill_quads(rect, &points, opaque_rgba([255, 80, 40, 20]));
        assert!(!fill.is_empty());
        assert!(fill.iter().all(|quad| quad.color[3] == 1.0));

        let edge = line_quad(rect, points[0], points[1], ACCENT, 2.0);
        let a = screen_point(rect, points[0]);
        let b = screen_point(rect, points[1]);
        assert!((edge.rect[0] + edge.rect[2] * 0.5 - (a.0 + b.0) * 0.5).abs() < 0.01);
        assert!((edge.rect[1] + edge.rect[3] * 0.5 - (a.1 + b.1) * 0.5).abs() < 0.01);
    }

    #[test]
    fn single_click_selects_and_drag_moves_without_editing_text() {
        let mut project = project();
        let page_id = project.active_page_id().unwrap();
        let bubble_id = project
            .add_bubble(
                page_id,
                vec![
                    Point { x: 0.2, y: 0.2 },
                    Point { x: 0.4, y: 0.2 },
                    Point { x: 0.3, y: 0.4 },
                ],
            )
            .unwrap();
        let layout = ComicDubsLayout::compute(Rect {
            x: 0.0,
            y: 0.0,
            width: 1_200.0,
            height: 800.0,
        });
        let page = image_rect(layout.canvas, project.active_page().unwrap());
        let start = screen_point(page, Point { x: 0.3, y: 0.28 });
        let mut ui = ComicDubsWorkspaceUi::default();

        assert_eq!(
            ui.handle_event(
                &UiEvent::MousePress {
                    x: start.0,
                    y: start.1,
                },
                &project,
                layout,
            ),
            EventResponse::Consumed
        );
        assert_eq!(ui.selected_bubble(), Some(bubble_id));
        assert!(!ui.is_editing_text());
        ui.handle_event(
            &UiEvent::MouseMove {
                x: start.0 + page.width * 0.1,
                y: start.1 + page.height * 0.1,
            },
            &project,
            layout,
        );
        assert!(matches!(
            ui.handle_event(
                &UiEvent::MouseRelease {
                    x: start.0 + page.width * 0.1,
                    y: start.1 + page.height * 0.1,
                },
                &project,
                layout,
            ),
            EventResponse::Action(UiAction::ComicDubsSetBubblePoints {
                bubble_id: id,
                points
            }) if id == bubble_id && points[0].x > 0.29 && points[0].y > 0.29
        ));
        assert_eq!(
            ui.handle_event(&UiEvent::Delete, &project, layout),
            EventResponse::Action(UiAction::ComicDubsRemoveBubble(bubble_id))
        );
    }

    #[test]
    fn selected_vertices_move_independently_and_empty_canvas_deselects() {
        let mut project = project();
        let page_id = project.active_page_id().unwrap();
        let bubble_id = project
            .add_bubble(
                page_id,
                vec![
                    Point { x: 0.2, y: 0.2 },
                    Point { x: 0.5, y: 0.2 },
                    Point { x: 0.35, y: 0.5 },
                ],
            )
            .unwrap();
        let layout = ComicDubsLayout::compute(Rect {
            x: 0.0,
            y: 0.0,
            width: 1_200.0,
            height: 800.0,
        });
        let page = image_rect(layout.canvas, project.active_page().unwrap());
        let center = screen_point(page, Point { x: 0.35, y: 0.3 });
        let vertex = screen_point(page, Point { x: 0.2, y: 0.2 });
        let mut ui = ComicDubsWorkspaceUi::default();
        ui.handle_event(
            &UiEvent::MousePress {
                x: center.0,
                y: center.1,
            },
            &project,
            layout,
        );
        ui.handle_event(
            &UiEvent::MouseRelease {
                x: center.0,
                y: center.1,
            },
            &project,
            layout,
        );
        ui.handle_event(
            &UiEvent::MousePress {
                x: vertex.0,
                y: vertex.1,
            },
            &project,
            layout,
        );
        ui.handle_event(
            &UiEvent::MouseMove {
                x: vertex.0 + page.width * 0.05,
                y: vertex.1 + page.height * 0.05,
            },
            &project,
            layout,
        );
        assert!(matches!(
            ui.handle_event(
                &UiEvent::MouseRelease {
                    x: vertex.0 + page.width * 0.05,
                    y: vertex.1 + page.height * 0.05,
                },
                &project,
                layout,
            ),
            EventResponse::Action(UiAction::ComicDubsSetBubblePoints { bubble_id: id, points })
                if id == bubble_id && points[0].x > 0.24 && points[1].x == 0.5
        ));

        ui.begin_text_edit(bubble_id, "Texte".into());
        assert_eq!(
            ui.handle_event(
                &UiEvent::MousePress {
                    x: page.x + page.width * 0.9,
                    y: page.y + page.height * 0.9,
                },
                &project,
                layout,
            ),
            EventResponse::Consumed
        );
        assert_eq!(ui.selected_bubble(), None);
        assert!(!ui.is_editing_text());
    }

    #[test]
    fn edited_text_renders_above_an_opaque_bubble_at_its_own_size() {
        let mut project = project();
        let page_id = project.active_page_id().unwrap();
        let bubble_id = project
            .add_bubble(
                page_id,
                vec![
                    Point { x: 0.2, y: 0.2 },
                    Point { x: 0.8, y: 0.2 },
                    Point { x: 0.8, y: 0.8 },
                    Point { x: 0.2, y: 0.8 },
                ],
            )
            .unwrap();
        project.set_bubble_font_size(bubble_id, 18.0);
        project.set_settings(Some("Arial".into()), 250, 250, 24.0);
        let mut ui = ComicDubsWorkspaceUi::default();
        ui.begin_text_edit(bubble_id, "Nouveau texte".into());
        let scene = ui.scene(
            &project,
            ComicDubsLayout::compute(Rect {
                x: 0.0,
                y: 0.0,
                width: 1_200.0,
                height: 800.0,
            }),
        );
        assert!(scene
            .labels
            .iter()
            .all(|label| label.text != "Nouveau texte|"));
        assert!(scene
            .overlay_labels
            .iter()
            .any(|label| label.text == "Nouveau texte|"
                && label.font_size <= 18.0
                && label.font_family.as_deref() == Some("Arial")));
        assert!(scene.overlay_quads.iter().any(|quad| quad.color[3] == 1.0));
    }

    #[test]
    fn inspector_edits_selected_size_and_opens_the_shared_color_picker() {
        let mut project = project();
        let page_id = project.active_page_id().unwrap();
        let bubble_id = project
            .add_bubble(
                page_id,
                vec![
                    Point { x: 0.2, y: 0.2 },
                    Point { x: 0.8, y: 0.2 },
                    Point { x: 0.5, y: 0.8 },
                ],
            )
            .unwrap();
        let layout = ComicDubsLayout::compute(Rect {
            x: 0.0,
            y: 0.0,
            width: 1_200.0,
            height: 800.0,
        });
        let mut ui = ComicDubsWorkspaceUi {
            selected_bubble: Some(bubble_id),
            ..Default::default()
        };
        let plus = inspector_plus(layout, 88.0);
        assert!(matches!(
            ui.handle_event(
                &UiEvent::MousePress {
                    x: plus.x + 2.0,
                    y: plus.y + 2.0,
                },
                &project,
                layout,
            ),
            EventResponse::Action(UiAction::ComicDubsSetBubbleFontSize {
                bubble_id: id,
                font_size: 26.0,
            }) if id == bubble_id
        ));
        let color = inspector_color(layout);
        assert_eq!(
            ui.handle_event(
                &UiEvent::MousePress {
                    x: color.x + 2.0,
                    y: color.y + 2.0,
                },
                &project,
                layout,
            ),
            EventResponse::Consumed
        );
        assert!(ui.color_picker.active);
        let origin = ui.color_picker.origin;
        assert!(matches!(
            ui.handle_event(
                &UiEvent::MousePress {
                    x: origin.0 + 80.0,
                    y: origin.1 + 80.0,
                },
                &project,
                layout,
            ),
            EventResponse::Action(UiAction::ComicDubsSetBubbleColor {
                bubble_id: id,
                ..
            }) if id == bubble_id
        ));
    }

    #[test]
    fn playback_reveals_bubbles_in_reading_order() {
        let mut project = project();
        let page_id = project.active_page_id().unwrap();
        for text in ["Première", "Deuxième"] {
            let id = project
                .add_bubble(
                    page_id,
                    vec![
                        Point { x: 0.2, y: 0.2 },
                        Point { x: 0.8, y: 0.2 },
                        Point { x: 0.5, y: 0.8 },
                    ],
                )
                .unwrap();
            project.set_bubble_text(id, text.into());
        }
        let layout = ComicDubsLayout::compute(Rect {
            x: 0.0,
            y: 0.0,
            width: 1_200.0,
            height: 800.0,
        });
        let mut ui = ComicDubsWorkspaceUi::default();
        ui.set_playback(Some(page_id), 1);
        let first = ui.scene(&project, layout);
        assert!(first
            .overlay_labels
            .iter()
            .any(|label| label.text == "Première"));
        assert!(first
            .overlay_labels
            .iter()
            .all(|label| label.text != "Deuxième"));

        ui.set_playback(Some(page_id), 2);
        let second = ui.scene(&project, layout);
        assert!(second
            .overlay_labels
            .iter()
            .any(|label| label.text == "Deuxième"));
    }

    #[test]
    fn editing_badges_show_reading_order_and_audio_status_only_while_editing() {
        let mut project = project();
        let page_id = project.active_page_id().unwrap();
        let points = vec![
            Point { x: 0.2, y: 0.2 },
            Point { x: 0.8, y: 0.2 },
            Point { x: 0.5, y: 0.8 },
        ];
        let first = project.add_bubble(page_id, points.clone()).unwrap();
        project.add_bubble(page_id, points).unwrap();
        let audio = project.add_audio(
            "line.wav".into(),
            "line.flac".into(),
            RecordedAudio {
                file_name: "line.flac".into(),
                sample_rate: 48_000,
                channels: 1,
                sample_count: 48_000,
                checksum: "a".repeat(40),
                waveform: WaveformData::default(),
            },
        );
        project.assign_audio(first, Some(audio));
        let layout = ComicDubsLayout::compute(Rect {
            x: 0.0,
            y: 0.0,
            width: 1_200.0,
            height: 800.0,
        });
        let mut ui = ComicDubsWorkspaceUi::default();
        let editing = ui.scene(&project, layout);
        assert!(editing.overlay_labels.iter().any(|label| label.text == "1"));
        assert!(editing.overlay_labels.iter().any(|label| label.text == "2"));

        let mut plain_badge = ComicDubsScene::default();
        render_reading_order_badge(
            &mut plain_badge,
            Rect {
                width: 400.0,
                height: 400.0,
                ..Rect::default()
            },
            &[
                Point { x: 0.1, y: 0.1 },
                Point { x: 0.9, y: 0.1 },
                Point { x: 0.5, y: 0.9 },
            ],
            1,
            false,
        );
        let mut audio_badge = ComicDubsScene::default();
        render_reading_order_badge(
            &mut audio_badge,
            Rect {
                width: 400.0,
                height: 400.0,
                ..Rect::default()
            },
            &[
                Point { x: 0.1, y: 0.1 },
                Point { x: 0.9, y: 0.1 },
                Point { x: 0.5, y: 0.9 },
            ],
            1,
            true,
        );
        assert!(audio_badge.overlay_quads.len() > plain_badge.overlay_quads.len());

        ui.set_playback(Some(page_id), 2);
        let playback = ui.scene(&project, layout);
        assert!(playback
            .overlay_labels
            .iter()
            .all(|label| label.text != "1" && label.text != "2"));
    }

    #[test]
    fn audio_drag_assigns_and_right_click_plays_the_drop_target_bubble() {
        let mut project = project();
        let page_id = project.active_page_id().unwrap();
        let bubble_id = project
            .add_bubble(
                page_id,
                vec![
                    Point { x: 0.2, y: 0.2 },
                    Point { x: 0.8, y: 0.2 },
                    Point { x: 0.8, y: 0.8 },
                    Point { x: 0.2, y: 0.8 },
                ],
            )
            .unwrap();
        let audio_id = project.add_audio(
            "line.wav".into(),
            "line.flac".into(),
            RecordedAudio {
                file_name: "line.flac".into(),
                sample_rate: 48_000,
                channels: 1,
                sample_count: 48_000,
                checksum: "a".repeat(40),
                waveform: WaveformData::default(),
            },
        );
        let layout = ComicDubsLayout::compute(Rect {
            x: 0.0,
            y: 0.0,
            width: 1_200.0,
            height: 800.0,
        });
        let page_rect = image_rect(layout.canvas, project.active_page().unwrap());
        let row = media_row(layout, 0);
        let mut ui = ComicDubsWorkspaceUi {
            media_tab: MediaTab::Audios,
            ..Default::default()
        };
        assert_eq!(
            ui.handle_event(
                &UiEvent::MousePress {
                    x: row.x + 10.0,
                    y: row.y + 10.0,
                },
                &project,
                layout,
            ),
            EventResponse::Consumed
        );
        assert!(matches!(
            ui.handle_event(
                &UiEvent::MouseRelease {
                    x: page_rect.x + page_rect.width * 0.5,
                    y: page_rect.y + page_rect.height * 0.5,
                },
                &project,
                layout,
            ),
            EventResponse::Action(UiAction::ComicDubsAssignAudio {
                bubble_id: id,
                audio_id: Some(audio)
            }) if id == bubble_id && audio == audio_id
        ));
        project.assign_audio(bubble_id, Some(audio_id));
        assert!(matches!(
            ui.handle_event(
                &UiEvent::ContextMenu {
                    x: page_rect.x + page_rect.width * 0.5,
                    y: page_rect.y + page_rect.height * 0.5,
                },
                &project,
                layout,
            ),
            EventResponse::Action(UiAction::ComicDubsAssignAudio {
                bubble_id: id,
                audio_id: Some(audio)
            }) if id == bubble_id && audio == audio_id
        ));
    }

    #[test]
    fn text_box_and_wrapping_stay_inside_a_triangular_bubble() {
        let triangle = [
            Point { x: 0.5, y: 0.1 },
            Point { x: 0.9, y: 0.9 },
            Point { x: 0.1, y: 0.9 },
        ];
        let page = Rect {
            x: 0.0,
            y: 0.0,
            width: 1_000.0,
            height: 1_000.0,
        };
        let bounds = polygon_text_bounds(page, &triangle);
        for (x, y) in [
            (bounds.x, bounds.y),
            (bounds.x + bounds.width, bounds.y),
            (bounds.x, bounds.y + bounds.height),
            (bounds.x + bounds.width, bounds.y + bounds.height),
        ] {
            assert!(point_in_polygon(point_at(page, x, y), &triangle));
        }
        let text = "Une traduction assez longue doit rester lisible dans la bulle";
        let (lines, font) = fit_text(text, bounds, 34.0);
        assert_eq!(lines.join(" "), text);
        assert!(lines.len() as f32 * font * 1.18 <= bounds.height);
    }
}
