//! Text-emotion layer over the established rythmo output adapter.

use super::{badge_policy, view_implementation};

#[path = "view_adapter.rs"]
mod base;

#[allow(hidden_glob_reexports)]
pub use base::*;

use std::collections::HashMap;

use crate::lint::Severity;
use crate::project::Project;
use crate::render_index::ProjectRenderIndex;
use crate::ui::primitives::{
    EventResponse, IconInstance, LabelInfo, QuadInstance, Rect, UiAction, UiEvent,
};
use crate::ui::renderer::StretchedText;
use crate::ui::ToolMode;

#[allow(clippy::too_many_arguments)]
pub fn render_lines<'a>(
    zone: &Rect,
    project: &'a Project,
    render_index: &ProjectRenderIndex,
    current_frame: f64,
    karaoke_preview: bool,
    fps: f64,
    state: &RythmoState,
    lint_severities: &HashMap<u64, Severity>,
    quads: &mut Vec<QuadInstance>,
    syllable_quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    stretched: &mut Vec<StretchedText>,
    note_icons: &mut Vec<IconInstance>,
    actor_icons: &mut Vec<VoiceActorIconDraw>,
    note_uv: [f32; 4],
    detection_uvs: [[f32; 4]; 18],
) -> Option<(
    u64,
    usize,
    Option<(usize, usize)>,
    f32,
    f32,
    f32,
    f32,
    Option<Vec<CursorSegmentInfo>>,
)> {
    let stretched_start = stretched.len();
    let result = base::render_lines(
        zone,
        project,
        render_index,
        current_frame,
        karaoke_preview,
        fps,
        state,
        lint_severities,
        quads,
        syllable_quads,
        labels,
        stretched,
        note_icons,
        actor_icons,
        note_uv,
        detection_uvs,
    );

    for draw in &mut stretched[stretched_start..] {
        let Some(line) = project.get_line(draw.line_id) else {
            continue;
        };
        if line.kind.is_dialogue()
            && !line.karaoke
            && draw.text == line.text
            && crate::text_emotion::has_line_for_text(line.id, &line.text)
        {
            draw.text = crate::text_emotion::encode_render_text(line.id, &draw.text);
        }
    }

    crate::text_emotion_foreground::render(quads, labels);
    result
}

#[allow(clippy::too_many_arguments)]
pub fn handle_rythmo_event(
    event: &UiEvent,
    zone: &Rect,
    project: &Project,
    render_index: &ProjectRenderIndex,
    current_frame: f64,
    karaoke_preview: bool,
    fps: f64,
    state: &mut RythmoState,
    tool_mode: ToolMode,
    brush_color: [f32; 4],
    brush_radius_frac: f32,
    erasing: bool,
    interaction_mode: RythmoInteractionMode,
) -> EventResponse {
    if let Some(response) = crate::text_emotion_foreground::handle_modal_event(event) {
        return response;
    }
    base::handle_rythmo_event(
        event,
        zone,
        project,
        render_index,
        current_frame,
        karaoke_preview,
        fps,
        state,
        tool_mode,
        brush_color,
        brush_radius_frac,
        erasing,
        interaction_mode,
    )
}

pub fn handle_context_menu_event(
    event: &UiEvent,
    project: &Project,
    current_frame: f64,
    zone: &Rect,
    screen_w: f32,
    screen_h: f32,
    state: &mut RythmoState,
) -> EventResponse {
    if let Some(response) = crate::text_emotion_foreground::handle_modal_event(event) {
        return response;
    }

    let response = base::handle_context_menu_event(
        event,
        project,
        current_frame,
        zone,
        screen_w,
        screen_h,
        state,
    );

    if matches!(event, UiEvent::ContextMenu { .. }) {
        let menu_target = state
            .context_menu
            .as_ref()
            .map(|menu| (menu.line_id, menu.x, menu.y));
        if let Some((line_id, x, y)) = menu_target {
            if crate::text_emotion_foreground::open_context_parent(
                project,
                state,
                line_id,
                x,
                y,
                screen_w,
                screen_h,
            ) {
                state.context_menu = None;
                return EventResponse::Actions(vec![
                    UiAction::Accessibility(crate::accessibility::AccessibilityEvent::Opened {
                        label: "Menu contextuel. Émotion du texte, sous-menu.".to_string(),
                    }),
                    UiAction::Accessibility(crate::accessibility::AccessibilityEvent::Focus {
                        label: "Émotion du texte".to_string(),
                        role: "menu button".to_string(),
                    }),
                ]);
            }
        }
    }

    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_dialogue_body_uses_emotion_metadata() {
        crate::text_emotion::clear();
        let mut project = Project::new();
        let dialogue = project.add_line_full(
            0,
            24,
            0.25,
            "Bonjour".to_string(),
            "Alice".to_string(),
            [1.0; 4],
        );
        crate::text_emotion::apply_range(
            dialogue,
            "Bonjour",
            0,
            7,
            Some(crate::text_emotion::TextEmotion::Wave),
        );
        assert!(crate::text_emotion::has_line_for_text(dialogue, "Bonjour"));
        assert!(project.get_line(dialogue).unwrap().kind.is_dialogue());
    }
}
