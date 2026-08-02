use super::primitives::{HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiEvent, VAlign};
use crate::i18n::t;
use crate::media_recording::{InputDeviceInfo, InputDeviceIssue};

const CARD_W: f32 = 620.0;
const CARD_H: f32 = 500.0;
const ROW_H: f32 = 42.0;

#[derive(Debug, PartialEq, Eq)]
pub enum MicrophoneModalResult {
    Consumed,
    Close,
    Select(Option<String>),
}

pub struct MicrophoneModal {
    devices: Vec<InputDeviceInfo>,
    display_labels: Vec<String>,
    selected: usize,
    scroll: usize,
    cancel_focused: bool,
}

impl MicrophoneModal {
    pub fn new(devices: Vec<InputDeviceInfo>, selected: Option<String>) -> Self {
        let selected = selected
            .as_deref()
            .and_then(|selected| devices.iter().position(|device| device.name == selected))
            .map_or(0, |index| index + 1);
        Self {
            display_labels: std::iter::once(t("recording.microphone.default").to_string())
                .chain(devices.iter().map(|device| {
                    device
                        .issue
                        .as_ref()
                        .map(|issue| format!("{} — {}", device.name, Self::issue_label(issue)))
                        .unwrap_or_else(|| device.name.clone())
                }))
                .collect(),
            devices,
            selected,
            scroll: 0,
            cancel_focused: false,
        }
    }

    pub fn keyboard_focus_label(&self) -> String {
        if self.cancel_focused {
            t("recording.microphone.cancel").to_string()
        } else {
            self.option_label(self.selected).to_string()
        }
    }

    fn card(sw: f32, sh: f32) -> Rect {
        let width = CARD_W.min((sw - 24.0).max(360.0));
        let height = CARD_H.min((sh - 24.0).max(260.0));
        Rect {
            x: (sw - width) / 2.0,
            y: (sh - height) / 2.0,
            width,
            height,
        }
    }

    fn list(card: Rect) -> Rect {
        Rect {
            x: card.x + 24.0,
            y: card.y + 96.0,
            width: card.width - 48.0,
            height: (card.height - 166.0).max(ROW_H),
        }
    }

    fn cancel(card: Rect) -> Rect {
        Rect {
            x: card.x + card.width - 154.0,
            y: card.y + card.height - 54.0,
            width: 130.0,
            height: 34.0,
        }
    }

    fn option_count(&self) -> usize {
        self.devices.len() + 1
    }

    fn visible_rows(list: Rect) -> usize {
        (list.height / ROW_H).floor().max(1.0) as usize
    }

    fn option_label(&self, index: usize) -> &str {
        self.display_labels
            .get(index)
            .map(String::as_str)
            .unwrap_or_default()
    }

    fn issue_label(issue: &InputDeviceIssue) -> String {
        match issue {
            InputDeviceIssue::DefaultConfigUnavailable => {
                t("recording.microphone.reason.no_config").to_string()
            }
            InputDeviceIssue::SupportedConfigUnavailable => {
                t("recording.microphone.reason.no_supported_config").to_string()
            }
            InputDeviceIssue::SampleRateTooLow(rate) => {
                t("recording.microphone.reason.sample_rate")
                    .replace("{rate}", &format!("{rate} Hz"))
            }
        }
    }

    fn selected_value(&self) -> Option<String> {
        self.selected
            .checked_sub(1)
            .and_then(|index| self.devices.get(index))
            .map(|device| device.name.clone())
    }

    fn ensure_selected_visible(&mut self, visible_rows: usize) {
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + visible_rows {
            self.scroll = self.selected + 1 - visible_rows;
        }
        self.scroll = self
            .scroll
            .min(self.option_count().saturating_sub(visible_rows));
    }

    pub fn handle_event(&mut self, event: &UiEvent, sw: f32, sh: f32) -> MicrophoneModalResult {
        let card = Self::card(sw, sh);
        let list = Self::list(card);
        let visible_rows = Self::visible_rows(list);
        match event {
            UiEvent::KeyInput { text } if text == "\x1b" => MicrophoneModalResult::Close,
            UiEvent::KeyInput { text } if text == "\t" || text == "\u{b}" => {
                self.cancel_focused = !self.cancel_focused;
                MicrophoneModalResult::Consumed
            }
            UiEvent::CursorUp if !self.cancel_focused => {
                self.selected = self.selected.saturating_sub(1);
                self.ensure_selected_visible(visible_rows);
                MicrophoneModalResult::Consumed
            }
            UiEvent::CursorDown if !self.cancel_focused => {
                self.selected = (self.selected + 1).min(self.option_count() - 1);
                self.ensure_selected_visible(visible_rows);
                MicrophoneModalResult::Consumed
            }
            UiEvent::Home if !self.cancel_focused => {
                self.selected = 0;
                self.ensure_selected_visible(visible_rows);
                MicrophoneModalResult::Consumed
            }
            UiEvent::End if !self.cancel_focused => {
                self.selected = self.option_count() - 1;
                self.ensure_selected_visible(visible_rows);
                MicrophoneModalResult::Consumed
            }
            UiEvent::KeyInput { text } if text == "\r" || text == "\n" => {
                if self.cancel_focused {
                    MicrophoneModalResult::Close
                } else {
                    MicrophoneModalResult::Select(self.selected_value())
                }
            }
            UiEvent::Scroll { x, y, delta, .. } if list.contains(*x, *y) => {
                if *delta < 0.0 {
                    self.scroll =
                        (self.scroll + 1).min(self.option_count().saturating_sub(visible_rows));
                } else if *delta > 0.0 {
                    self.scroll = self.scroll.saturating_sub(1);
                }
                MicrophoneModalResult::Consumed
            }
            UiEvent::MousePress { x, y }
            | UiEvent::DoubleClick { x, y }
            | UiEvent::CtrlClick { x, y }
            | UiEvent::ShiftMousePress { x, y } => {
                if !card.contains(*x, *y) || Self::cancel(card).contains(*x, *y) {
                    return MicrophoneModalResult::Close;
                }
                if list.contains(*x, *y) {
                    let index = self.scroll + ((*y - list.y) / ROW_H).floor() as usize;
                    if index < self.option_count() {
                        self.selected = index;
                        return MicrophoneModalResult::Select(self.selected_value());
                    }
                }
                MicrophoneModalResult::Consumed
            }
            _ => MicrophoneModalResult::Consumed,
        }
    }

    pub fn render<'a>(
        &'a self,
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        sw: f32,
        sh: f32,
    ) {
        let card = Self::card(sw, sh);
        let list = Self::list(card);
        push_panel(
            quads,
            Rect {
                x: 0.0,
                y: 0.0,
                width: sw,
                height: sh,
            },
            [0.015, 0.015, 0.025, 0.82],
            0.0,
            [0.0; 4],
        );
        push_panel(
            quads,
            card,
            [0.12, 0.12, 0.16, 1.0],
            14.0,
            [0.38, 0.38, 0.48, 0.9],
        );
        labels.push(label(
            t("recording.microphone.title"),
            Rect {
                x: card.x + 24.0,
                y: card.y + 18.0,
                width: card.width - 48.0,
                height: 30.0,
            },
            20.0,
            HAlign::Left,
            Some([240, 240, 248]),
        ));
        labels.push(label(
            t("recording.microphone.description"),
            Rect {
                x: card.x + 24.0,
                y: card.y + 52.0,
                width: card.width - 48.0,
                height: 32.0,
            },
            12.0,
            HAlign::Left,
            Some([170, 174, 190]),
        ));
        push_panel(
            quads,
            list,
            [0.07, 0.075, 0.095, 1.0],
            8.0,
            [0.22, 0.25, 0.32, 1.0],
        );

        for index in self.scroll..(self.scroll + Self::visible_rows(list)).min(self.option_count())
        {
            let row = Rect {
                x: list.x + 4.0,
                y: list.y + (index - self.scroll) as f32 * ROW_H + 3.0,
                width: list.width - 8.0,
                height: ROW_H - 6.0,
            };
            if index == self.selected && !self.cancel_focused {
                push_panel(
                    quads,
                    row,
                    [0.18, 0.32, 0.58, 1.0],
                    6.0,
                    [0.38, 0.62, 1.0, 1.0],
                );
            }
            let incompatible = index > 0
                && self
                    .devices
                    .get(index - 1)
                    .is_some_and(|device| device.issue.is_some());
            labels.push(LabelInfo {
                text: self.option_label(index),
                bounds: row,
                h_align: HAlign::Left,
                v_align: VAlign::Center,
                overflow: Overflow::Ellipsis,
                padding: 12.0,
                font_size_override: Some(14.0),
                color_override: Some(if incompatible {
                    [235, 92, 92]
                } else {
                    [230, 232, 240]
                }),
                font_family_override: None,
            });
        }

        let cancel = Self::cancel(card);
        push_panel(
            quads,
            cancel,
            [0.20, 0.20, 0.27, 1.0],
            7.0,
            if self.cancel_focused {
                [0.38, 0.62, 1.0, 1.0]
            } else {
                [0.36, 0.38, 0.46, 0.8]
            },
        );
        labels.push(label(
            t("recording.microphone.cancel"),
            cancel,
            13.0,
            HAlign::Center,
            Some([235, 235, 242]),
        ));
    }
}

#[derive(Debug, PartialEq)]
pub enum RecordingActorMenuResult {
    Consumed,
    Close,
    ChooseMicrophone,
    SetVideoVolume(f32),
}

pub struct RecordingActorMenuModal {
    volume: f32,
    volume_label: String,
    focused: usize,
    dragging_volume: bool,
}

impl RecordingActorMenuModal {
    pub fn new(volume: f32) -> Self {
        let volume = volume.clamp(0.0, 1.0);
        Self {
            volume,
            volume_label: volume_percent(volume),
            focused: 0,
            dragging_volume: false,
        }
    }

    pub fn keyboard_focus_label(&self) -> String {
        match self.focused {
            0 => t("recording.actor_menu.microphone").to_string(),
            1 => format!(
                "{}: {}",
                t("recording.actor_menu.video_volume"),
                self.volume_label
            ),
            _ => t("recording.actor_menu.return").to_string(),
        }
    }

    pub fn keyboard_focus_role(&self) -> &'static str {
        if self.focused == 1 {
            "slider"
        } else {
            "button"
        }
    }

    fn card(sw: f32, sh: f32) -> Rect {
        let width = 560.0_f32.min((sw - 24.0).max(360.0));
        let height = 330.0_f32.min((sh - 24.0).max(280.0));
        Rect {
            x: (sw - width) / 2.0,
            y: (sh - height) / 2.0,
            width,
            height,
        }
    }

    fn microphone_button(card: Rect) -> Rect {
        Rect {
            x: card.x + 30.0,
            y: card.y + 92.0,
            width: card.width - 60.0,
            height: 42.0,
        }
    }

    fn volume_slider(card: Rect) -> Rect {
        Rect {
            x: card.x + 42.0,
            y: card.y + 180.0,
            width: card.width - 84.0,
            height: 28.0,
        }
    }

    fn close_button(card: Rect) -> Rect {
        Rect {
            x: card.x + card.width - 174.0,
            y: card.y + card.height - 54.0,
            width: 144.0,
            height: 34.0,
        }
    }

    fn set_volume(&mut self, volume: f32) -> RecordingActorMenuResult {
        self.volume = volume.clamp(0.0, 1.0);
        self.volume_label = volume_percent(self.volume);
        RecordingActorMenuResult::SetVideoVolume(self.volume)
    }

    fn volume_from_x(slider: Rect, x: f32) -> f32 {
        ((x - slider.x) / slider.width).clamp(0.0, 1.0)
    }

    pub fn handle_event(&mut self, event: &UiEvent, sw: f32, sh: f32) -> RecordingActorMenuResult {
        let card = Self::card(sw, sh);
        let slider = Self::volume_slider(card);
        match event {
            UiEvent::KeyInput { text } if text == "\x1b" => RecordingActorMenuResult::Close,
            UiEvent::KeyInput { text } if text == "\t" || text == "\u{b}" => {
                self.focused = if text == "\t" {
                    (self.focused + 1) % 3
                } else {
                    (self.focused + 2) % 3
                };
                RecordingActorMenuResult::Consumed
            }
            UiEvent::CursorUp => {
                self.focused = (self.focused + 2) % 3;
                RecordingActorMenuResult::Consumed
            }
            UiEvent::CursorDown => {
                self.focused = (self.focused + 1) % 3;
                RecordingActorMenuResult::Consumed
            }
            UiEvent::CursorLeft if self.focused == 1 => self.set_volume(self.volume - 0.05),
            UiEvent::CursorRight if self.focused == 1 => self.set_volume(self.volume + 0.05),
            UiEvent::Home if self.focused == 1 => self.set_volume(0.0),
            UiEvent::End if self.focused == 1 => self.set_volume(1.0),
            UiEvent::KeyInput { text } if text == "\r" || text == "\n" => match self.focused {
                0 => RecordingActorMenuResult::ChooseMicrophone,
                2 => RecordingActorMenuResult::Close,
                _ => RecordingActorMenuResult::Consumed,
            },
            UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
                if !card.contains(*x, *y) || Self::close_button(card).contains(*x, *y) {
                    return RecordingActorMenuResult::Close;
                }
                if Self::microphone_button(card).contains(*x, *y) {
                    return RecordingActorMenuResult::ChooseMicrophone;
                }
                if slider.contains(*x, *y) {
                    self.focused = 1;
                    self.dragging_volume = true;
                    return self.set_volume(Self::volume_from_x(slider, *x));
                }
                RecordingActorMenuResult::Consumed
            }
            UiEvent::MouseMove { x, .. } if self.dragging_volume => {
                self.set_volume(Self::volume_from_x(slider, *x))
            }
            UiEvent::MouseRelease { .. } if self.dragging_volume => {
                self.dragging_volume = false;
                RecordingActorMenuResult::Consumed
            }
            UiEvent::Scroll { x, y, delta, .. } if slider.contains(*x, *y) => {
                self.set_volume(self.volume + delta.signum() * 0.05)
            }
            _ => RecordingActorMenuResult::Consumed,
        }
    }

    pub fn render<'a>(
        &'a self,
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        sw: f32,
        sh: f32,
    ) {
        let card = Self::card(sw, sh);
        let microphone = Self::microphone_button(card);
        let slider = Self::volume_slider(card);
        let close = Self::close_button(card);
        push_panel(
            quads,
            Rect {
                x: 0.0,
                y: 0.0,
                width: sw,
                height: sh,
            },
            [0.015, 0.015, 0.025, 0.82],
            0.0,
            [0.0; 4],
        );
        push_panel(
            quads,
            card,
            [0.12, 0.12, 0.16, 1.0],
            14.0,
            [0.38, 0.38, 0.48, 0.9],
        );
        labels.push(label(
            t("recording.actor_menu.title"),
            Rect {
                x: card.x + 30.0,
                y: card.y + 18.0,
                width: card.width - 60.0,
                height: 30.0,
            },
            20.0,
            HAlign::Left,
            Some([240, 240, 248]),
        ));
        labels.push(label(
            t("recording.actor_menu.description"),
            Rect {
                x: card.x + 30.0,
                y: card.y + 50.0,
                width: card.width - 60.0,
                height: 28.0,
            },
            12.0,
            HAlign::Left,
            Some([170, 174, 190]),
        ));
        push_panel(
            quads,
            microphone,
            [0.18, 0.22, 0.34, 1.0],
            7.0,
            if self.focused == 0 {
                [0.38, 0.62, 1.0, 1.0]
            } else {
                [0.36, 0.38, 0.46, 0.8]
            },
        );
        labels.push(label(
            t("recording.actor_menu.microphone"),
            microphone,
            14.0,
            HAlign::Center,
            Some([235, 235, 242]),
        ));
        labels.push(label(
            t("recording.actor_menu.video_volume"),
            Rect {
                x: slider.x,
                y: slider.y - 28.0,
                width: slider.width - 70.0,
                height: 20.0,
            },
            13.0,
            HAlign::Left,
            Some([215, 218, 230]),
        ));
        labels.push(LabelInfo {
            text: &self.volume_label,
            bounds: Rect {
                x: slider.x + slider.width - 70.0,
                y: slider.y - 28.0,
                width: 70.0,
                height: 20.0,
            },
            h_align: HAlign::Right,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(13.0),
            color_override: Some([215, 218, 230]),
            font_family_override: None,
        });
        let track = Rect {
            x: slider.x,
            y: slider.y + 11.0,
            width: slider.width,
            height: 6.0,
        };
        push_panel(
            quads,
            track,
            [0.07, 0.075, 0.095, 1.0],
            3.0,
            if self.focused == 1 {
                [0.38, 0.62, 1.0, 1.0]
            } else {
                [0.22, 0.25, 0.32, 1.0]
            },
        );
        push_panel(
            quads,
            Rect {
                width: track.width * self.volume,
                ..track
            },
            [0.34, 0.48, 0.88, 1.0],
            3.0,
            [0.0; 4],
        );
        push_panel(
            quads,
            Rect {
                x: track.x + track.width * self.volume - 7.0,
                y: track.y - 4.0,
                width: 14.0,
                height: 14.0,
            },
            [0.85, 0.87, 0.95, 1.0],
            7.0,
            [0.0; 4],
        );
        labels.push(label(
            t("recording.actor_menu.video_volume_hint"),
            Rect {
                x: slider.x,
                y: slider.y + 30.0,
                width: slider.width,
                height: 20.0,
            },
            11.0,
            HAlign::Left,
            Some([145, 150, 168]),
        ));
        push_panel(
            quads,
            close,
            [0.20, 0.20, 0.27, 1.0],
            7.0,
            if self.focused == 2 {
                [0.38, 0.62, 1.0, 1.0]
            } else {
                [0.36, 0.38, 0.46, 0.8]
            },
        );
        labels.push(label(
            t("recording.actor_menu.return"),
            close,
            13.0,
            HAlign::Center,
            Some([235, 235, 242]),
        ));
    }
}

fn volume_percent(volume: f32) -> String {
    format!("{} %", (volume.clamp(0.0, 1.0) * 100.0).round())
}

fn push_panel(
    quads: &mut Vec<QuadInstance>,
    rect: Rect,
    color: [f32; 4],
    radius: f32,
    border: [f32; 4],
) {
    quads.push(QuadInstance {
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
    });
}

fn label(
    text: &str,
    bounds: Rect,
    font_size: f32,
    h_align: HAlign,
    color: Option<[u8; 3]>,
) -> LabelInfo<'_> {
    LabelInfo {
        text,
        bounds,
        h_align,
        v_align: VAlign::Center,
        overflow: Overflow::Clip,
        padding: 0.0,
        font_size_override: Some(font_size),
        color_override: color,
        font_family_override: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyboard_selects_the_highlighted_microphone() {
        let mut modal = MicrophoneModal::new(
            vec![InputDeviceInfo {
                name: "Studio microphone".into(),
                issue: None,
            }],
            None,
        );
        assert_eq!(
            modal.handle_event(&UiEvent::CursorDown, 1280.0, 720.0),
            MicrophoneModalResult::Consumed
        );
        assert_eq!(
            modal.handle_event(&UiEvent::KeyInput { text: "\r".into() }, 1280.0, 720.0),
            MicrophoneModalResult::Select(Some("Studio microphone".into()))
        );
    }

    #[test]
    fn listed_microphones_are_selectable_even_when_preflight_reports_an_issue() {
        let mut modal = MicrophoneModal::new(
            vec![InputDeviceInfo {
                name: "Telephone microphone".into(),
                issue: Some(InputDeviceIssue::SampleRateTooLow(16_000)),
            }],
            None,
        );
        let card = MicrophoneModal::card(1280.0, 720.0);
        let list = MicrophoneModal::list(card);
        assert_eq!(
            modal.handle_event(
                &UiEvent::MousePress {
                    x: list.x + 20.0,
                    y: list.y + ROW_H + 10.0,
                },
                1280.0,
                720.0,
            ),
            MicrophoneModalResult::Select(Some("Telephone microphone".into()))
        );
        assert!(modal.option_label(1).contains("48 kHz"));
    }

    #[test]
    fn mouse_click_selects_a_microphone_for_any_local_user() {
        let mut modal = MicrophoneModal::new(
            vec![InputDeviceInfo {
                name: "Studio microphone".into(),
                issue: None,
            }],
            None,
        );
        let card = MicrophoneModal::card(1280.0, 720.0);
        let list = MicrophoneModal::list(card);
        assert_eq!(
            modal.handle_event(
                &UiEvent::MousePress {
                    x: list.x + 20.0,
                    y: list.y + ROW_H + 10.0,
                },
                1280.0,
                720.0,
            ),
            MicrophoneModalResult::Select(Some("Studio microphone".into()))
        );
    }

    #[test]
    fn actor_menu_adjusts_video_volume_from_the_keyboard() {
        let mut modal = RecordingActorMenuModal::new(0.75);
        assert_eq!(
            modal.handle_event(&UiEvent::CursorDown, 1280.0, 720.0),
            RecordingActorMenuResult::Consumed
        );
        assert_eq!(
            modal.handle_event(&UiEvent::CursorLeft, 1280.0, 720.0),
            RecordingActorMenuResult::SetVideoVolume(0.70)
        );
    }
}
