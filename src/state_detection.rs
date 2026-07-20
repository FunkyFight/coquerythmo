//! Semantic detection use cases attached to the application state.

use crate::application::edit_service::{EditExecutor, EditOrigin};
use crate::command::Command;
use crate::detection::{
    DetectionAddress, DetectionChange, DetectionCue, DetectionKind, MediaTick, TextAnchor,
};
use crate::state::State;
use crate::workspaces::rythmo::view::Selection;

impl State {
    pub fn has_selected_detection(&self) -> bool {
        matches!(
            self.ui_shell.ui.rythmo_state.selected,
            Some(Selection::Detection(_))
        )
    }

    pub fn rythmo_detection_hovered(&self) -> bool {
        self.ui_shell.ui.rythmo_state.detection_hover.is_some()
    }

    pub fn open_detection_palette_from_hover(&mut self) -> bool {
        self.ui_shell
            .ui
            .rythmo_state
            .open_detection_palette_from_hover()
    }

    pub fn focus_detection_parent_line(&mut self) -> bool {
        let Some(Selection::Detection(address)) = self.ui_shell.ui.rythmo_state.selected else {
            return false;
        };
        self.ui_shell.ui.rythmo_state.selected = Some(Selection::Line(address.line_id));
        self.ui_shell.ui.rythmo_state.detection_drag = None;
        true
    }

    pub fn add_detection(
        &mut self,
        line_id: u64,
        kind: DetectionKind,
        media_tick: MediaTick,
        target: TextAnchor,
    ) {
        if self.project_session.project.get_line(line_id).is_none() || target.validate().is_err() {
            return;
        }
        let detection_id = self
            .project_session
            .project
            .detections()
            .line(line_id)
            .and_then(|line| line.next_detection_id())
            .unwrap_or(crate::detection::DetectionCueId(1));
        let address = DetectionAddress {
            line_id,
            detection_id,
        };
        let change = DetectionChange::Add {
            address,
            cue: DetectionCue {
                id: detection_id,
                kind,
                media_tick,
                target,
            },
        };
        EditExecutor::execute(
            &mut self.project_session,
            Command::Detection { change },
            EditOrigin::Local,
        );
        self.ui_shell.ui.rythmo_state.selected = Some(Selection::Detection(address));
        self.ui_shell.ui.rythmo_state.detection_menu = None;
    }

    pub fn move_detection(&mut self, address: DetectionAddress, media_tick: MediaTick) {
        let Some(old_tick) = self
            .project_session
            .project
            .detections()
            .detection(address)
            .map(|cue| cue.media_tick)
        else {
            return;
        };
        if old_tick == media_tick {
            return;
        }
        let change = DetectionChange::Move {
            address,
            old_tick,
            new_tick: media_tick,
        };
        let coalesce = matches!(
            self.project_session.history.last(),
            Some(Command::Detection {
                change: DetectionChange::Move {
                    address: previous_address,
                    ..
                }
            }) if *previous_address == address
        );
        let command = Command::Detection {
            change: change.clone(),
        };
        if coalesce {
            EditExecutor::coalesce(
                &mut self.project_session,
                command,
                |last| {
                    if let Command::Detection {
                        change: DetectionChange::Move { new_tick, .. },
                    } = last
                    {
                        *new_tick = media_tick;
                    }
                },
                EditOrigin::Local,
            );
        } else {
            EditExecutor::execute(&mut self.project_session, command, EditOrigin::Local);
        }
        self.ui_shell.ui.rythmo_state.selected = Some(Selection::Detection(address));
    }

    pub fn delete_detection(&mut self, address: DetectionAddress) {
        let Some(cue) = self
            .project_session
            .project
            .detections()
            .detection(address)
            .cloned()
        else {
            return;
        };
        EditExecutor::execute(
            &mut self.project_session,
            Command::Detection {
                change: DetectionChange::Remove { address, cue },
            },
            EditOrigin::Local,
        );
        self.ui_shell.ui.rythmo_state.selected = Some(Selection::Line(address.line_id));
        self.ui_shell.ui.rythmo_state.detection_drag = None;
    }

    pub fn delete_selected_detection(&mut self) {
        let Some(Selection::Detection(address)) = self.ui_shell.ui.rythmo_state.selected else {
            return;
        };
        self.delete_detection(address);
    }

    pub fn nudge_selected_detection(&mut self, delta_ticks: i64) {
        let Some(Selection::Detection(address)) = self.ui_shell.ui.rythmo_state.selected else {
            return;
        };
        let Some(current) = self
            .project_session
            .project
            .detections()
            .detection(address)
            .map(|cue| cue.media_tick)
        else {
            return;
        };
        let Some(line) = self.project_session.project.get_line(address.line_id) else {
            return;
        };
        let min = MediaTick::from_frame(line.start_frame);
        let max = MediaTick::from_frame(line.end_frame());
        let next = MediaTick(current.raw().saturating_add(delta_ticks)).clamp(min, max);
        self.move_detection(address, next);
    }
}
