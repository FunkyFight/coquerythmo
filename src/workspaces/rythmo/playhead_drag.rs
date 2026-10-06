//! Moving the reading bar with the mouse: the bar itself is grabbed and
//! dragged; a double click on it puts it back in the middle.

use super::*;

/// Extra grab distance on each side of the bar.
const BAR_HIT_PADDING: f32 = 4.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayheadDrag {
    /// Offset when the drag started, restored if it is cancelled.
    pub original_percent: f32,
    /// Pointer distance from the bar's centre when it was grabbed.
    pub grab_dx: f32,
    pub percent: f32,
}

/// Horizontal centre of the reading bar for an offset in percent.
pub(crate) fn playhead_center_x(zone: &Rect, offset_percent: f32) -> f32 {
    zone.x + zone.width / 2.0 - offset_percent / 100.0 * zone.width
}

/// Offset in percent that puts the bar's centre at `x`, rounded to a tenth
/// and kept within the project setting range.
pub(crate) fn offset_percent_for_x(zone: &Rect, x: f32) -> f32 {
    if zone.width <= 0.0 {
        return 0.0;
    }
    let percent = (zone.x + zone.width / 2.0 - x) / zone.width * 100.0;
    ((percent * 10.0).round() / 10.0).clamp(-50.0, 50.0)
}

fn handle_hit(zone: &Rect, project: &Project, offset_percent: f32, x: f32, y: f32) -> bool {
    let center = playhead_center_x(zone, offset_percent);
    let reach = project.settings().band_style.playhead_width / 2.0 + BAR_HIT_PADDING;
    (x - center).abs() <= reach && y >= zone.y && y <= zone.y + zone.height
}

/// Reading bar colour, brightened while it is hovered or dragged.
pub(crate) fn playhead_display_color(project: &Project, state: &RythmoState) -> [f32; 4] {
    let base = project.settings().band_style.playhead;
    if state.playhead_drag.is_none() && !state.playhead_handle_hover {
        return base;
    }
    [
        (base[0] + 0.25).min(1.0),
        (base[1] + 0.25).min(1.0),
        (base[2] + 0.25).min(1.0),
        base[3],
    ]
}

/// Pointer handling for the reading bar handle. Returns `None` for events it
/// lets through.
pub(crate) fn handle_playhead_drag_event(
    event: &UiEvent,
    zone: &Rect,
    project: &Project,
    state: &mut RythmoState,
) -> Option<EventResponse> {
    let current = project.settings().reading_bar_offset_percent;
    match event {
        UiEvent::MousePress { x, y }
            if !state.audio_offset_mode && handle_hit(zone, project, current, *x, *y) =>
        {
            state.playhead_drag = Some(PlayheadDrag {
                original_percent: current,
                grab_dx: *x - playhead_center_x(zone, current),
                percent: current,
            });
            state.playhead_handle_hover = true;
            Some(EventResponse::Consumed)
        }
        UiEvent::DoubleClick { x, y }
            if !state.audio_offset_mode && handle_hit(zone, project, current, *x, *y) =>
        {
            // Double click puts the bar back in the middle of the band.
            state.playhead_drag = None;
            Some(EventResponse::Action(UiAction::CommitReadingBarOffset {
                percent: 0.0,
                original_percent: current,
            }))
        }
        UiEvent::MouseMove { x, y } => {
            let Some(drag) = state.playhead_drag.as_mut() else {
                state.playhead_handle_hover =
                    !state.audio_offset_mode && handle_hit(zone, project, current, *x, *y);
                return None;
            };
            let percent = offset_percent_for_x(zone, *x - drag.grab_dx);
            if percent == drag.percent {
                return Some(EventResponse::Consumed);
            }
            drag.percent = percent;
            Some(EventResponse::Action(UiAction::PreviewReadingBarOffset(
                percent,
            )))
        }
        UiEvent::MouseRelease { .. } => {
            let drag = state.playhead_drag.take()?;
            Some(EventResponse::Action(UiAction::CommitReadingBarOffset {
                percent: drag.percent,
                original_percent: drag.original_percent,
            }))
        }
        UiEvent::KeyInput { text } if text == "\x1b" => {
            let drag = state.playhead_drag.take()?;
            Some(EventResponse::Action(UiAction::PreviewReadingBarOffset(
                drag.original_percent,
            )))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zone() -> Rect {
        Rect {
            x: 100.0,
            y: 500.0,
            width: 1000.0,
            height: 300.0,
        }
    }

    #[test]
    fn offset_and_position_are_inverse() {
        let zone = zone();
        for percent in [-50.0, -12.5, 0.0, 20.0, 50.0] {
            let x = playhead_center_x(&zone, percent);
            assert!((offset_percent_for_x(&zone, x) - percent).abs() < 0.051);
        }
        assert_eq!(offset_percent_for_x(&zone, -5000.0), 50.0);
        assert_eq!(offset_percent_for_x(&zone, 5000.0), -50.0);
    }

    #[test]
    fn dragging_the_handle_previews_then_commits() {
        let zone = zone();
        let project = Project::new();
        let mut state = RythmoState::default();
        let center = playhead_center_x(&zone, 0.0);
        let press = UiEvent::MousePress {
            x: center + 2.0,
            y: zone.y + 5.0,
        };
        assert_eq!(
            handle_playhead_drag_event(&press, &zone, &project, &mut state),
            Some(EventResponse::Consumed)
        );
        let moved = UiEvent::MouseMove {
            x: center + 2.0 - 100.0,
            y: zone.y + 5.0,
        };
        assert_eq!(
            handle_playhead_drag_event(&moved, &zone, &project, &mut state),
            Some(EventResponse::Action(UiAction::PreviewReadingBarOffset(
                10.0
            )))
        );
        let release = UiEvent::MouseRelease { x: 0.0, y: 0.0 };
        assert_eq!(
            handle_playhead_drag_event(&release, &zone, &project, &mut state),
            Some(EventResponse::Action(UiAction::CommitReadingBarOffset {
                percent: 10.0,
                original_percent: 0.0,
            }))
        );
        assert!(state.playhead_drag.is_none());
    }

    #[test]
    fn the_bar_is_grabbed_along_its_whole_height_only() {
        let zone = zone();
        let project = Project::new();
        let mut state = RythmoState::default();
        let beside = UiEvent::MousePress {
            x: playhead_center_x(&zone, 0.0) + 20.0,
            y: zone.y + 100.0,
        };
        assert_eq!(
            handle_playhead_drag_event(&beside, &zone, &project, &mut state),
            None
        );
        let on_bar = UiEvent::MousePress {
            x: playhead_center_x(&zone, 0.0) + 1.0,
            y: zone.y + 250.0,
        };
        assert_eq!(
            handle_playhead_drag_event(&on_bar, &zone, &project, &mut state),
            Some(EventResponse::Consumed)
        );
    }

    #[test]
    fn double_click_on_the_bar_recentres_it() {
        let zone = zone();
        let mut project = Project::new();
        let mut settings = project.settings().clone();
        settings.reading_bar_offset_percent = 20.0;
        project.set_settings(settings);
        let mut state = RythmoState::default();
        let double = UiEvent::DoubleClick {
            x: playhead_center_x(&zone, 20.0),
            y: zone.y + 150.0,
        };
        assert_eq!(
            handle_playhead_drag_event(&double, &zone, &project, &mut state),
            Some(EventResponse::Action(UiAction::CommitReadingBarOffset {
                percent: 0.0,
                original_percent: 20.0,
            }))
        );
    }
}
