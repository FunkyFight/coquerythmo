from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file_path = Path(path)
    text = file_path.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise RuntimeError(f"Expected exactly one match in {path}, found {count}: {old[:120]!r}")
    file_path.write_text(text.replace(old, new, 1), encoding="utf-8")


# Hide the unfinished workspace completely outside developer builds.
replace_once(
    "src/ui/shell.rs",
    """pub(crate) fn build_workspace_tabs(
    layout: &Layout,
    active_workspace: WorkspaceId,
) -> Vec<Box<dyn Widget>> {
    let tab_width = 164.0;
""",
    """pub(crate) fn build_workspace_tabs(
    layout: &Layout,
    active_workspace: WorkspaceId,
) -> Vec<Box<dyn Widget>> {
    if !crate::config::dev_mode() {
        return Vec::new();
    }

    let tab_width = 164.0;
""",
)

# Do not reserve an empty tab strip when the developer-only workspace is hidden.
replace_once(
    "src/ui/layout.rs",
    """        let main_w = screen_w - props_w;
        let content_top = TOPBAR_H + TABBAR_H;
        let content_h = screen_h - content_top;
""",
    """        let main_w = screen_w - props_w;
        let tabbar_h = if crate::config::dev_mode() {
            TABBAR_H
        } else {
            0.0
        };
        let content_top = TOPBAR_H + tabbar_h;
        let content_h = screen_h - content_top;
""",
)
replace_once(
    "src/ui/layout.rs",
    """        let tabs = Rect {
            x: 0.0,
            y: TOPBAR_H,
            width: screen_w,
            height: TABBAR_H,
        };
""",
    """        let tabs = Rect {
            x: 0.0,
            y: TOPBAR_H,
            width: screen_w,
            height: tabbar_h,
        };
""",
)
replace_once(
    "src/ui/layout.rs",
    """        assert_eq!(layout.tabs.height, TABBAR_H);
        assert_eq!(layout.properties.unwrap().y, TOPBAR_H + TABBAR_H);
        assert_eq!(layout.video_preview.y, TOPBAR_H + TABBAR_H);
""",
    """        assert_eq!(layout.tabs.height, 0.0);
        assert_eq!(layout.properties.unwrap().y, TOPBAR_H);
        assert_eq!(layout.video_preview.y, TOPBAR_H);
""",
)

# Reject every recording/mini-DAW command at the application boundary when disabled.
replace_once(
    "src/app/dispatcher.rs",
    """    ) -> bool {
        if state.active_workspace() == crate::application::workspace_service::WorkspaceId::Recording
            && action.mutates_rythmo_project()
""",
    """    ) -> bool {
        if !config::dev_mode()
            && matches!(
                &action,
                UiAction::ActivateWorkspace(
                    crate::application::workspace_service::WorkspaceId::Recording
                )
                    | UiAction::RecordingChooseSolo
                    | UiAction::RecordingChooseOnline
                    | UiAction::RecordingSetTool(_)
                    | UiAction::RecordingToggleTrackMute(_)
                    | UiAction::RecordingToggleTrackSolo(_)
                    | UiAction::RecordingArmTrack(_)
                    | UiAction::RecordingSelectClip { .. }
                    | UiAction::RecordingSelectAsset(_)
                    | UiAction::RecordingStartCapture
                    | UiAction::RecordingStopCapture
            )
        {
            return false;
        }

        if state.active_workspace() == crate::application::workspace_service::WorkspaceId::Recording
            && action.mutates_rythmo_project()
""",
)

# A karaoke line may become centered only once its own ball count-in has begun.
replace_once(
    "src/workspaces/rythmo/view.rs",
    """        let karaoke_upcoming_stack =
            karaoke_playback && karaoke_index.upcoming_stack_visible(line, current_frame);
""",
    """        let karaoke_upcoming_stack = karaoke_playback
            && karaoke_count_in
            && karaoke_index.upcoming_stack_visible(line, current_frame);
""",
)

# The karaoke layout only has two physical rows. When very tight/overlapping timings
# produce more than two candidates, keep the newest line assigned to each row instead
# of drawing two text runs at exactly the same coordinates.
replace_once(
    "src/workspaces/rythmo/view.rs",
    """    // Keep a stable vertical draw order, then compare every badge with the
    // actual body of the other visible lines.
""",
    """    let mut latest_karaoke_by_row: HashMap<(usize, usize), (i64, u64)> = HashMap::new();
    for (line_id, data) in &line_data {
        if !data.karaoke_playback {
            continue;
        }
        let Some(line) = project.get_line(*line_id) else {
            continue;
        };
        let key = (
            rythmo_layout::track_index_for_y_slot(line.y_slot),
            karaoke_index.stack_row(line),
        );
        let candidate = (line.start_frame, line.id);
        latest_karaoke_by_row
            .entry(key)
            .and_modify(|current| *current = (*current).max(candidate))
            .or_insert(candidate);
    }
    line_data.retain(|(line_id, data)| {
        if !data.karaoke_playback {
            return true;
        }
        let Some(line) = project.get_line(*line_id) else {
            return false;
        };
        let key = (
            rythmo_layout::track_index_for_y_slot(line.y_slot),
            karaoke_index.stack_row(line),
        );
        latest_karaoke_by_row.get(&key).copied() == Some((line.start_frame, line.id))
    });

    // Keep a stable vertical draw order, then compare every badge with the
    // actual body of the other visible lines.
""",
)
