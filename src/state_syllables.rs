//! Keyboard handling of a karaoke line: a cursor walks its syllable handles
//! and, under them, its points and liaisons.

use crate::accessibility::AccessibilityEvent;
use crate::application::command::{SyllableKey, UiAction};
use crate::application::workspace_service::WorkspaceId;
use crate::i18n::t;
use crate::state::State;
use crate::workspaces::rythmo::view::{
    self as rythmo, Selection, SyllableCursor, SyllableMark, SyllableTarget,
};

/// What the keyboard can reach on a karaoke line.
struct LineSyllables {
    line_id: u64,
    start_frame: i64,
    duration_frames: i64,
    /// Text of every syllable.
    texts: Vec<String>,
    /// Start and end of every syllable, as shares of the line.
    boundaries: Vec<f32>,
    /// Points and liaisons under the line, in reading order.
    marks: Vec<SyllableMark>,
    /// Where each of them stands, as a share of the line.
    mark_shares: Vec<f32>,
    /// What each of them is read as: its word and its state.
    mark_labels: Vec<String>,
}

impl LineSyllables {
    /// Handles of the line: one at the start of every syllable but the first.
    fn handles(&self) -> usize {
        self.texts.len().saturating_sub(1)
    }

    fn frame_at(&self, share: f32) -> i64 {
        self.start_frame + (share as f64 * self.duration_frames as f64).round() as i64
    }

    /// The target, brought back to something that still exists.
    fn valid(&self, target: SyllableTarget) -> Option<SyllableTarget> {
        match target {
            SyllableTarget::Handle(_) if self.handles() == 0 => None,
            SyllableTarget::Handle(syllable) => {
                Some(SyllableTarget::Handle(syllable.clamp(1, self.handles())))
            }
            SyllableTarget::Mark(_) if self.marks.is_empty() => None,
            SyllableTarget::Mark(mark) => {
                Some(SyllableTarget::Mark(mark.min(self.marks.len() - 1)))
            }
        }
    }

    /// The handle nearest to `share`.
    fn handle_near(&self, share: f32) -> Option<SyllableTarget> {
        (1..=self.handles())
            .min_by(|a, b| {
                (self.boundaries[*a] - share)
                    .abs()
                    .total_cmp(&(self.boundaries[*b] - share).abs())
            })
            .map(SyllableTarget::Handle)
    }

    /// The point or liaison nearest to `share`.
    fn mark_near(&self, share: f32) -> Option<SyllableTarget> {
        (0..self.marks.len())
            .min_by(|a, b| {
                (self.mark_shares[*a] - share)
                    .abs()
                    .total_cmp(&(self.mark_shares[*b] - share).abs())
            })
            .map(SyllableTarget::Mark)
    }

    /// Where the target stands, as a share of the line.
    fn share_of(&self, target: SyllableTarget) -> f32 {
        match target {
            SyllableTarget::Handle(syllable) => self.boundaries.get(syllable).copied(),
            SyllableTarget::Mark(mark) => self.mark_shares.get(mark).copied(),
        }
        .unwrap_or(0.0)
    }
}

impl State {
    /// The karaoke line the keyboard can work on: the only selected line,
    /// while nothing is typed.
    fn syllable_keys_line(&self) -> Option<u64> {
        if self.active_workspace() != WorkspaceId::Rythmo
            || self.is_editing_text()
            || self.captures_modal_input()
        {
            return None;
        }
        let Some(Selection::Line(id)) = self.ui_shell.ui.rythmo_state().selected.as_ref() else {
            return None;
        };
        let line = self.project_session.project.get_line(*id)?;
        (line.karaoke && !line.kind.is_ambiance() && !line.text.trim().is_empty()).then_some(*id)
    }

    pub fn syllable_keys_available(&self) -> bool {
        self.syllable_keys_line().is_some()
    }

    fn syllable_cursor(&self) -> Option<SyllableCursor> {
        let line_id = self.syllable_keys_line()?;
        self.ui_shell
            .ui
            .rythmo_state()
            .active_syllable_cursor()
            .filter(|cursor| cursor.line_id == line_id)
    }

    pub fn has_syllable_cursor(&self) -> bool {
        self.syllable_cursor().is_some()
    }

    fn line_syllables(&self, line_id: u64) -> Option<LineSyllables> {
        let project = &self.project_session.project;
        let state = self.ui_shell.ui.rythmo_state();
        let line = project.get_line(line_id)?;
        let lang = project.syllable_language_code();
        let marks = rythmo::syllable_marks(line, lang);
        let mark_labels = marks
            .iter()
            .map(|mark| match mark {
                SyllableMark::Pin(pin) => t(if pin.active {
                    "accessibility.syllable_split_on"
                } else {
                    "accessibility.syllable_split_off"
                })
                .replace("{word}", &pin.word),
                SyllableMark::Liaison(site) => t(match line.liaison_state(&site.pair) {
                    Some(true) => "accessibility.liaison_made",
                    Some(false) => "accessibility.liaison_not_made",
                    None => "accessibility.liaison_unmarked",
                })
                .replace("{pair}", &site.pair),
            })
            .collect();
        Some(LineSyllables {
            line_id,
            start_frame: line.start_frame,
            duration_frames: line.duration_frames,
            texts: rythmo::syllable_char_ranges(line, lang, state)
                .into_iter()
                .map(|range| rythmo::syllable_text(line, range))
                .collect(),
            boundaries: rythmo::syllable_boundary_shares(project, line, lang, state),
            mark_shares: rythmo::syllable_mark_shares(project, line, lang, state, &marks),
            marks,
            mark_labels,
        })
    }

    fn spoken_syllable_time(&self, frame: i64) -> String {
        crate::ui::recording_workspace::spoken_duration(frame.max(0) as f64 / self.fps().max(1.0))
    }

    fn syllable_say(&self, message: impl Into<String>) {
        self.announce_accessibility(AccessibilityEvent::Success {
            message: message.into(),
        });
    }

    /// Puts the cursor on `target` and reads it out: "Poignée 2 sur 4, avant
    /// tres, 1 minute 2 secondes" or "Point 1 sur 3, monstres : syllabes
    /// prononcées".
    fn syllable_select(&mut self, line: &LineSyllables, target: SyllableTarget) {
        self.ui_shell.ui.rythmo_state.syllable_cursor = Some(SyllableCursor {
            line_id: line.line_id,
            target,
        });
        self.request_redraw();
        let label = match target {
            SyllableTarget::Handle(syllable) => {
                let mut label = t("accessibility.syllable_cursor")
                    .replace("{index}", &syllable.to_string())
                    .replace("{count}", &line.handles().to_string())
                    .replace("{text}", &line.texts[syllable])
                    .replace(
                        "{time}",
                        &self.spoken_syllable_time(line.frame_at(line.boundaries[syllable])),
                    );
                let placed = rythmo::syllable_handle_is_placed(
                    self.ui_shell.ui.rythmo_state(),
                    line.line_id,
                    syllable,
                    line.texts.len(),
                );
                if placed {
                    label.push_str(", ");
                    label.push_str(t("accessibility.syllable_placed"));
                }
                label
            }
            SyllableTarget::Mark(mark) => t("accessibility.syllable_mark")
                .replace(
                    "{kind}",
                    t(match line.marks[mark] {
                        SyllableMark::Pin(_) => "accessibility.syllable_mark_pin",
                        SyllableMark::Liaison(_) => "accessibility.syllable_mark_liaison",
                    }),
                )
                .replace("{index}", &(mark + 1).to_string())
                .replace("{count}", &line.marks.len().to_string())
                .replace("{state}", &line.mark_labels[mark]),
        };
        self.announce_accessibility(AccessibilityEvent::Selection { label });
    }

    /// Share of the line under the playhead, when it is over the line.
    fn syllable_playhead_share(&self, line: &LineSyllables) -> Option<f32> {
        let frame = self.current_frame();
        (line.duration_frames > 0
            && frame >= line.start_frame
            && frame < line.start_frame + line.duration_frames)
            .then(|| (frame - line.start_frame) as f32 / line.duration_frames as f32)
    }

    pub fn syllable_key(&mut self, key: SyllableKey) {
        let Some(line) = self
            .syllable_keys_line()
            .and_then(|line_id| self.line_syllables(line_id))
        else {
            self.syllable_say(t("accessibility.syllable_need_line"));
            return;
        };
        let current = self
            .syllable_cursor()
            .and_then(|cursor| line.valid(cursor.target));
        match key {
            SyllableKey::Step(direction) => {
                let target = match current {
                    Some(SyllableTarget::Handle(syllable)) => {
                        Some(SyllableTarget::Handle(if direction < 0 {
                            syllable.saturating_sub(1).max(1)
                        } else {
                            (syllable + 1).min(line.handles())
                        }))
                    }
                    Some(SyllableTarget::Mark(mark)) => {
                        Some(SyllableTarget::Mark(if direction < 0 {
                            mark.saturating_sub(1)
                        } else {
                            (mark + 1).min(line.marks.len() - 1)
                        }))
                    }
                    // The first press takes the handle of the syllable under
                    // the playhead, else the first one. A line without a
                    // handle still has its points and liaisons.
                    None => self
                        .syllable_playhead_share(&line)
                        .map(|share| rythmo::syllable_at_share(&line.boundaries, share))
                        .and_then(|syllable| line.valid(SyllableTarget::Handle(syllable)))
                        .or_else(|| line.valid(SyllableTarget::Handle(1)))
                        .or_else(|| line.valid(SyllableTarget::Mark(0))),
                };
                match target {
                    Some(target) => self.syllable_select(&line, target),
                    None => self.syllable_say(t("accessibility.syllable_nothing")),
                }
            }
            SyllableKey::Row(direction) => {
                let near = current
                    .map(|target| line.share_of(target))
                    .or_else(|| self.syllable_playhead_share(&line))
                    .unwrap_or(0.0);
                let target = if direction > 0 {
                    match current {
                        Some(SyllableTarget::Mark(_)) => current,
                        _ => line.mark_near(near),
                    }
                } else {
                    match current {
                        Some(SyllableTarget::Handle(_)) => current,
                        _ => line.handle_near(near),
                    }
                };
                match target {
                    Some(target) => self.syllable_select(&line, target),
                    None => self.syllable_say(t(if direction > 0 {
                        "accessibility.syllable_no_mark"
                    } else {
                        "accessibility.syllable_first"
                    })),
                }
            }
            SyllableKey::Leave => {
                if current.is_some() {
                    self.ui_shell.ui.rythmo_state.syllable_cursor = None;
                    self.request_redraw();
                    self.syllable_say(t("accessibility.syllable_cursor_left"));
                }
            }
            SyllableKey::Nudge(_) | SyllableKey::PlaceAtPlayhead => {
                let Some(SyllableTarget::Handle(syllable)) = current else {
                    self.syllable_say(t("accessibility.syllable_need_cursor"));
                    return;
                };
                let (target, advance) = match key {
                    SyllableKey::Nudge(delta_frames) => (
                        line.frame_at(line.boundaries[syllable]) + delta_frames as i64,
                        false,
                    ),
                    _ => (self.current_frame(), true),
                };
                self.syllable_move_handle(&line, syllable, target, advance);
            }
            SyllableKey::Activate => match current {
                Some(SyllableTarget::Handle(syllable)) => {
                    self.syllable_toggle_placed(&line, syllable)
                }
                Some(SyllableTarget::Mark(mark)) => {
                    match line.marks[mark].clone() {
                        SyllableMark::Pin(pin) => {
                            self.toggle_syllable_split_word(line.line_id, &pin.word)
                        }
                        SyllableMark::Liaison(site) => {
                            self.cycle_liaison_mark(line.line_id, &site.pair)
                        }
                    }
                    self.request_redraw();
                }
                None => self.syllable_say(t("accessibility.syllable_need_cursor")),
            },
        }
    }

    /// Moves the handle that starts `syllable` to `target_frame`. After a
    /// tap at the playhead the cursor goes to the next handle, ready for the
    /// next tap.
    fn syllable_move_handle(
        &mut self,
        line: &LineSyllables,
        syllable: usize,
        target_frame: i64,
        advance: bool,
    ) {
        let text = &line.texts[syllable];
        let moved = {
            let project = &self.project_session.project;
            let state = &mut self.ui_shell.ui.rythmo_state;
            project.get_line(line.line_id).and_then(|project_line| {
                if project_line.duration_frames <= 0 {
                    return None;
                }
                if target_frame <= project_line.start_frame
                    || target_frame >= project_line.end_frame()
                {
                    return Some(Err(()));
                }
                let target = (target_frame - project_line.start_frame) as f32
                    / project_line.duration_frames as f32;
                rythmo::move_syllable_start(project, state, project_line, syllable, target)
                    .map(|(actions, _)| Ok(actions))
            })
        };
        let actions = match moved {
            Some(Ok(actions)) => actions,
            Some(Err(())) => {
                self.syllable_say(t("accessibility.syllable_outside_line"));
                return;
            }
            None => {
                self.syllable_say(t("accessibility.syllable_blocked").replace("{text}", text));
                return;
            }
        };
        for action in actions {
            match action {
                UiAction::SetSyllableRatios { line_id, ratios } => {
                    self.set_syllable_ratios(line_id, ratios)
                }
                UiAction::ResizeLine {
                    id,
                    start_frame,
                    duration_frames,
                } => self.resize_line(id, start_frame, duration_frames),
                _ => {}
            }
        }
        let after = self.line_syllables(line.line_id);
        let reached = after
            .as_ref()
            .and_then(|line| {
                line.boundaries
                    .get(syllable)
                    .map(|share| line.frame_at(*share))
            })
            .unwrap_or(target_frame);
        let mut message = t("accessibility.syllable_moved")
            .replace("{text}", text)
            .replace("{time}", &self.spoken_syllable_time(reached));
        if advance && syllable < line.handles() {
            self.ui_shell.ui.rythmo_state.syllable_cursor = Some(SyllableCursor {
                line_id: line.line_id,
                target: SyllableTarget::Handle(syllable + 1),
            });
            message.push_str(". ");
            message.push_str(
                &t("accessibility.syllable_next").replace("{text}", &line.texts[syllable + 1]),
            );
        }
        self.request_redraw();
        self.syllable_say(message);
    }

    /// `Entrée` on a handle: places it where it stands (the next moves stop
    /// at it), or frees it.
    fn syllable_toggle_placed(&mut self, line: &LineSyllables, syllable: usize) {
        let count = line.texts.len();
        let state = &mut self.ui_shell.ui.rythmo_state;
        let key = if rythmo::syllable_handle_is_placed(state, line.line_id, syllable, count) {
            rythmo::free_syllable_handle(state, line.line_id, syllable);
            "accessibility.syllable_handle_freed"
        } else {
            state.place_syllable_handle(line.line_id, syllable - 1, count);
            "accessibility.syllable_handle_placed"
        };
        self.request_redraw();
        self.syllable_say(t(key).replace("{text}", &line.texts[syllable]));
    }
}
