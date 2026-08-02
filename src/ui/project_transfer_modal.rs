use super::primitives::{HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiEvent, VAlign};
use crate::i18n::t;
use crate::network::{ProjectTransferMetadata, ProjectTransferStatus};

const CARD_W: f32 = 650.0;
const CARD_H: f32 = 430.0;
const BUTTON_W: f32 = 190.0;
const BUTTON_H: f32 = 42.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectTransferAction {
    Accept,
    SaveAndReplace,
    Replace,
    Refuse,
}

pub struct ProjectTransferModal {
    pub metadata: ProjectTransferMetadata,
    pub status: Option<ProjectTransferStatus>,
    pub is_director: bool,
    pub dirty: bool,
    focused: usize,
    response_submitted: bool,
    row_labels: Vec<String>,
    phase_label: String,
    project_label: String,
    result_path: Option<String>,
}

impl ProjectTransferModal {
    pub fn new(metadata: ProjectTransferMetadata, is_director: bool, dirty: bool) -> Self {
        let project_label = format!(
            "{} — {:.1} Mio",
            metadata.file_name,
            metadata.total_bytes as f64 / 1_048_576.0
        );
        Self {
            metadata,
            status: None,
            is_director,
            dirty,
            focused: 0,
            response_submitted: false,
            row_labels: Vec::new(),
            phase_label: String::new(),
            project_label,
            result_path: None,
        }
    }

    pub fn set_status(&mut self, status: ProjectTransferStatus) {
        let percent = if status.total_bytes == 0 {
            0
        } else {
            (status.transferred_bytes.saturating_mul(100) / status.total_bytes).min(100)
        };
        self.phase_label = match status.phase.as_str() {
            "transferring" => format!(
                "{} - {percent} %",
                t("recording.project_transfer.receiving")
            ),
            "finishing" => t("recording.project_transfer.load_waiting").to_string(),
            "completed" => t("recording.project_transfer.complete").to_string(),
            "cancelled" => t("recording.project_transfer.failed").to_string(),
            _ => String::new(),
        };
        self.row_labels = status
            .participants
            .iter()
            .take(8)
            .map(|participant| {
                let countdown = participant
                    .deadline
                    .filter(|deadline| *deadline > 0)
                    .and_then(|deadline| {
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .ok()?
                            .as_millis() as u64;
                        Some(format!(
                            " ({} s)",
                            deadline.saturating_sub(now).div_ceil(1_000)
                        ))
                    })
                    .unwrap_or_default();
                let progress = if participant.response == "receiving" {
                    format!(" - {} %", (participant.progress * 100.0).round() as u32)
                } else {
                    String::new()
                };
                format!(
                    "{} - {}{}{}",
                    participant.username,
                    transfer_label(&participant.response),
                    progress,
                    countdown
                )
            })
            .collect();
        self.status = Some(status);
    }

    pub fn set_result_path(&mut self, path: String) {
        self.result_path = Some(path);
    }

    pub fn mark_response_submitted(&mut self) {
        self.response_submitted = true;
    }

    pub fn reset_response(&mut self) {
        self.response_submitted = false;
    }

    pub fn refresh_countdown(&mut self) {
        if let Some(status) = self.status.clone() {
            self.set_status(status);
        }
    }

    fn card_rect(sw: f32, sh: f32) -> Rect {
        Rect {
            x: (sw - CARD_W) / 2.0,
            y: (sh - CARD_H) / 2.0,
            width: CARD_W,
            height: CARD_H,
        }
    }

    fn buttons(&self, card: Rect) -> Vec<Rect> {
        let count: usize = if self.is_director {
            0
        } else if self.dirty {
            3
        } else {
            2
        };
        let gap = 10.0;
        let total = count as f32 * BUTTON_W + count.saturating_sub(1) as f32 * gap;
        let start = card.x + (card.width - total) / 2.0;
        (0..count)
            .map(|index| Rect {
                x: start + index as f32 * (BUTTON_W + gap),
                y: card.y + card.height - BUTTON_H - 22.0,
                width: BUTTON_W,
                height: BUTTON_H,
            })
            .collect()
    }

    pub fn handle_event(
        &mut self,
        event: &UiEvent,
        sw: f32,
        sh: f32,
    ) -> Option<ProjectTransferAction> {
        if self.is_director {
            return None;
        }
        if self.response_submitted {
            return None;
        }
        let buttons = self.buttons(Self::card_rect(sw, sh));
        match event {
            UiEvent::KeyInput { text } if text == "\x1b" => Some(ProjectTransferAction::Refuse),
            UiEvent::KeyInput { text } if text == "\t" => {
                self.focused = (self.focused + 1) % buttons.len();
                None
            }
            UiEvent::CursorRight | UiEvent::CursorDown => {
                self.focused = (self.focused + 1) % buttons.len();
                None
            }
            UiEvent::CursorLeft | UiEvent::CursorUp => {
                self.focused = (self.focused + buttons.len() - 1) % buttons.len();
                None
            }
            UiEvent::KeyInput { text } if text == "\r" || text == "\n" => {
                Some(self.action_for(self.focused))
            }
            UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => buttons
                .iter()
                .position(|button| button.contains(*x, *y))
                .map(|index| self.action_for(index)),
            _ => None,
        }
    }

    fn action_for(&self, index: usize) -> ProjectTransferAction {
        if self.dirty {
            match index {
                0 => ProjectTransferAction::SaveAndReplace,
                1 => ProjectTransferAction::Replace,
                _ => ProjectTransferAction::Refuse,
            }
        } else if index == 0 {
            ProjectTransferAction::Accept
        } else {
            ProjectTransferAction::Refuse
        }
    }

    pub fn render<'a>(
        &'a self,
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        sw: f32,
        sh: f32,
    ) {
        let card = Self::card_rect(sw, sh);
        quads.push(QuadInstance {
            rect: [0.0, 0.0, sw, sh],
            color: [0.0, 0.0, 0.0, 0.84],
            color_bottom: [0.0, 0.0, 0.0, 0.84],
            border_color: [0.0; 4],
            border_width: 0.0,
            border_radius: 0.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
        quads.push(QuadInstance {
            rect: [card.x, card.y, card.width, card.height],
            color: [0.16, 0.16, 0.20, 1.0],
            color_bottom: [0.10, 0.10, 0.14, 1.0],
            border_color: [0.42, 0.42, 0.52, 0.9],
            border_width: 1.0,
            border_radius: 16.0,
            shadow_offset: [0.0, 8.0],
            shadow_color: [0.0, 0.0, 0.0, 0.6],
            shadow_blur: 18.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
        labels.push(LabelInfo {
            text: t("recording.project_transfer.title"),
            bounds: Rect {
                x: card.x + 28.0,
                y: card.y + 20.0,
                width: card.width - 56.0,
                height: 34.0,
            },
            h_align: HAlign::Left,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(20.0),
            color_override: Some([248, 211, 99]),
            font_family_override: None,
        });
        let summary = if self.is_director {
            t("recording.project_transfer.waiting")
        } else {
            t("recording.project_transfer_request_received")
        };
        labels.push(LabelInfo {
            text: summary,
            bounds: Rect {
                x: card.x + 28.0,
                y: card.y + 62.0,
                width: card.width - 56.0,
                height: 30.0,
            },
            h_align: HAlign::Left,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(15.0),
            color_override: Some([225, 225, 235]),
            font_family_override: None,
        });
        if self.status.is_some() {
            let mut y = card.y + 104.0;
            for text in &self.row_labels {
                labels.push(LabelInfo {
                    text: text.as_str(),
                    bounds: Rect {
                        x: card.x + 32.0,
                        y,
                        width: card.width - 64.0,
                        height: 25.0,
                    },
                    h_align: HAlign::Left,
                    v_align: VAlign::Center,
                    overflow: Overflow::Clip,
                    padding: 0.0,
                    font_size_override: Some(14.0),
                    color_override: Some([210, 210, 220]),
                    font_family_override: None,
                });
                y += 28.0;
            }
        } else {
            labels.push(LabelInfo {
                text: self.project_label.as_str(),
                bounds: Rect {
                    x: card.x + 32.0,
                    y: card.y + 112.0,
                    width: card.width - 64.0,
                    height: 30.0,
                },
                h_align: HAlign::Left,
                v_align: VAlign::Center,
                overflow: Overflow::Clip,
                padding: 0.0,
                font_size_override: Some(15.0),
                color_override: Some([210, 210, 220]),
                font_family_override: None,
            });
        }
        if let Some(status) = &self.status {
            // Collecting responses is not file transfer yet.
            if matches!(status.phase.as_str(), "transferring" | "finishing") {
                let progress = if status.total_bytes == 0 {
                    0.0
                } else {
                    status.transferred_bytes as f32 / status.total_bytes as f32
                };
                let bar = Rect {
                    x: card.x + 32.0,
                    y: card.y + card.height - 100.0,
                    width: card.width - 64.0,
                    height: 14.0,
                };
                labels.push(LabelInfo {
                    text: self.phase_label.as_str(),
                    bounds: Rect {
                        x: bar.x,
                        y: bar.y - 27.0,
                        width: bar.width,
                        height: 22.0,
                    },
                    h_align: HAlign::Left,
                    v_align: VAlign::Center,
                    overflow: Overflow::Clip,
                    padding: 0.0,
                    font_size_override: Some(13.0),
                    color_override: Some([195, 205, 225]),
                    font_family_override: None,
                });
                quads.push(QuadInstance {
                    rect: [bar.x, bar.y, bar.width, bar.height],
                    color: [0.08, 0.08, 0.11, 1.0],
                    color_bottom: [0.08, 0.08, 0.11, 1.0],
                    border_color: [0.30, 0.30, 0.38, 0.8],
                    border_width: 1.0,
                    border_radius: 7.0,
                    shadow_offset: [0.0; 2],
                    shadow_color: [0.0; 4],
                    shadow_blur: 0.0,
                    rotation: 0.0,
                    _padding: [0.0; 2],
                });
                if progress > 0.0 {
                    quads.push(QuadInstance {
                        rect: [
                            bar.x + 2.0,
                            bar.y + 2.0,
                            (bar.width - 4.0) * progress.clamp(0.0, 1.0),
                            bar.height - 4.0,
                        ],
                        color: [0.35, 0.60, 1.0, 1.0],
                        color_bottom: [0.25, 0.45, 0.85, 1.0],
                        border_color: [0.0; 4],
                        border_width: 0.0,
                        border_radius: 5.0,
                        shadow_offset: [0.0; 2],
                        shadow_color: [0.0; 4],
                        shadow_blur: 0.0,
                        rotation: 0.0,
                        _padding: [0.0; 2],
                    });
                }
            }
        }
        if let Some(path) = &self.result_path {
            labels.push(LabelInfo {
                text: path.as_str(),
                bounds: Rect {
                    x: card.x + 32.0,
                    y: card.y + card.height - 126.0,
                    width: card.width - 64.0,
                    height: 22.0,
                },
                h_align: HAlign::Left,
                v_align: VAlign::Center,
                overflow: Overflow::Clip,
                padding: 0.0,
                font_size_override: Some(11.0),
                color_override: Some([180, 190, 205]),
                font_family_override: None,
            });
        }
        if self.response_submitted {
            return;
        }
        for (index, button) in self.buttons(card).iter().enumerate() {
            let text = if self.dirty {
                [
                    t("recording.project_transfer.save_replace"),
                    t("recording.project_transfer.replace"),
                    t("recording.project_transfer.refuse"),
                ][index]
            } else {
                [
                    t("recording.project_transfer.accept"),
                    t("recording.project_transfer.refuse"),
                ][index]
            };
            quads.push(QuadInstance {
                rect: [button.x, button.y, button.width, button.height],
                color: if self.focused == index {
                    [0.30, 0.46, 0.72, 1.0]
                } else {
                    [0.22, 0.25, 0.32, 1.0]
                },
                color_bottom: [0.12, 0.14, 0.20, 1.0],
                border_color: [0.45, 0.52, 0.70, 0.9],
                border_width: 1.0,
                border_radius: 8.0,
                shadow_offset: [0.0; 2],
                shadow_color: [0.0; 4],
                shadow_blur: 0.0,
                rotation: 0.0,
                _padding: [0.0; 2],
            });
            labels.push(LabelInfo {
                text,
                bounds: *button,
                h_align: HAlign::Center,
                v_align: VAlign::Center,
                overflow: Overflow::Clip,
                padding: 0.0,
                font_size_override: Some(14.0),
                color_override: Some([240, 240, 245]),
                font_family_override: None,
            });
        }
    }
}

fn transfer_label(response: &str) -> &'static str {
    match response {
        "saving" => t("recording.project_transfer.saving"),
        "accepted" => t("recording.project_transfer.accepted"),
        "refused" => t("recording.project_transfer.refused"),
        "expired" => t("recording.project_transfer.expired"),
        "receiving" => t("recording.project_transfer.receiving"),
        "loading" => t("recording.project_transfer.loading"),
        "loaded" => t("recording.project_transfer.loaded"),
        "failed" => t("recording.project_transfer.failed"),
        _ => t("recording.project_transfer.pending"),
    }
}
