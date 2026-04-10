use super::widget::{HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiEvent, VAlign};
use crate::i18n::t;

const CARD_W: f32 = 480.0;
const CARD_H: f32 = 280.0;
const OPT_H: f32 = 32.0;
const OPT_GAP: f32 = 8.0;

// --- Question types ---

#[derive(Clone, Copy, PartialEq)]
pub enum QContent { Dialogue, Musique, Podcast, Mixte }

#[derive(Clone, Copy, PartialEq)]
pub enum QQuality { Studio, Correct, Bruyant }

#[derive(Clone, Copy, PartialEq)]
pub enum QReverb { Aucun, Leger, Fort }

#[derive(Clone, Copy, PartialEq)]
pub enum QRetry { Premiere, PasAssez, TropAltere }

#[derive(Clone, Copy, PartialEq)]
pub enum QResult { VoixSeule, Instrumental, VoixFond }

#[derive(Clone, Copy, PartialEq)]
pub enum QDynamics { Constant, Variable, Comprime }

#[derive(Clone, Copy, PartialEq)]
pub enum WizardStep { Content, Quality, Reverb, Retry, Result, Dynamics }

const STEP_COUNT: usize = 6;

// --- Wizard state ---

pub struct VocalWizard {
    step: WizardStep,
    content: Option<QContent>,
    quality: Option<QQuality>,
    reverb: Option<QReverb>,
    retry: Option<QRetry>,
    result: Option<QResult>,
    dynamics: Option<QDynamics>,
    step_label: String,
}

pub enum WizardResult {
    Consumed,
    BackToManual { preset: [f32; 11] },
    Cancel,
}

impl VocalWizard {
    pub fn new() -> Self {
        Self {
            step: WizardStep::Content,
            content: None,
            quality: None,
            reverb: None,
            retry: None,
            result: None,
            dynamics: None,
            step_label: "1/6".to_string(),
        }
    }

    fn step_index(&self) -> usize {
        match self.step {
            WizardStep::Content => 0,
            WizardStep::Quality => 1,
            WizardStep::Reverb => 2,
            WizardStep::Retry => 3,
            WizardStep::Result => 4,
            WizardStep::Dynamics => 5,
        }
    }

    fn advance(&mut self) -> Option<WizardStep> {
        match self.step {
            WizardStep::Content if self.content.is_some() => Some(WizardStep::Quality),
            WizardStep::Quality if self.quality.is_some() => Some(WizardStep::Reverb),
            WizardStep::Reverb if self.reverb.is_some() => Some(WizardStep::Retry),
            WizardStep::Retry if self.retry.is_some() => Some(WizardStep::Result),
            WizardStep::Result if self.result.is_some() => Some(WizardStep::Dynamics),
            WizardStep::Dynamics if self.dynamics.is_some() => None, // done
            _ => None,
        }
    }

    fn go_back(&mut self) -> bool {
        match self.step {
            WizardStep::Content => false,
            WizardStep::Quality => { self.step = WizardStep::Content; true }
            WizardStep::Reverb => { self.step = WizardStep::Quality; true }
            WizardStep::Retry => { self.step = WizardStep::Reverb; true }
            WizardStep::Result => { self.step = WizardStep::Retry; true }
            WizardStep::Dynamics => { self.step = WizardStep::Result; true }
        }
    }

    fn card_rect(sw: f32, sh: f32) -> Rect {
        Rect { x: (sw - CARD_W) / 2.0, y: (sh - CARD_H) / 2.0, width: CARD_W, height: CARD_H }
    }

    pub fn handle_event(&mut self, event: &UiEvent, sw: f32, sh: f32) -> WizardResult {
        let card = Self::card_rect(sw, sh);
        match event {
            UiEvent::KeyInput { text } if text == "\x1b" => WizardResult::Cancel,

            UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
                if !card.contains(*x, *y) {
                    return WizardResult::Cancel;
                }

                let base_y = card.y + 56.0;
                let px = card.x + 30.0;

                // Check option clicks for current step
                let choices = self.current_choices();
                for (i, _) in choices.iter().enumerate() {
                    let ry = base_y + i as f32 * (OPT_H + OPT_GAP);
                    let opt_rect = Rect { x: px, y: ry, width: CARD_W - 60.0, height: OPT_H };
                    if opt_rect.contains(*x, *y) {
                        self.select_choice(i);
                        // If this is the last step, compute and return
                        if self.step == WizardStep::Dynamics && self.dynamics.is_some() {
                            let preset = compute_preset(self);
                            return WizardResult::BackToManual { preset };
                        }
                        // Otherwise advance
                        if let Some(next) = self.advance() {
                            self.step = next;
                            self.step_label = format!("{}/{}", self.step_index() + 1, STEP_COUNT);
                        }
                        return WizardResult::Consumed;
                    }
                }

                // Back button
                let back_btn = Rect { x: card.x + 30.0, y: card.y + CARD_H - 44.0, width: 100.0, height: 30.0 };
                if back_btn.contains(*x, *y) {
                    if !self.go_back() {
                        return WizardResult::Cancel;
                    }
                    self.step_label = format!("{}/{}", self.step_index() + 1, STEP_COUNT);
                    return WizardResult::Consumed;
                }

                // Cancel button
                let cancel_btn = Rect { x: card.x + CARD_W - 130.0, y: card.y + CARD_H - 44.0, width: 100.0, height: 30.0 };
                if cancel_btn.contains(*x, *y) {
                    return WizardResult::Cancel;
                }

                WizardResult::Consumed
            }
            _ => WizardResult::Consumed,
        }
    }

    fn select_choice(&mut self, index: usize) {
        match self.step {
            WizardStep::Content => {
                self.content = match index {
                    0 => Some(QContent::Dialogue),
                    1 => Some(QContent::Musique),
                    2 => Some(QContent::Podcast),
                    3 => Some(QContent::Mixte),
                    _ => None,
                };
            }
            WizardStep::Quality => {
                self.quality = match index {
                    0 => Some(QQuality::Studio),
                    1 => Some(QQuality::Correct),
                    2 => Some(QQuality::Bruyant),
                    _ => None,
                };
            }
            WizardStep::Reverb => {
                self.reverb = match index {
                    0 => Some(QReverb::Aucun),
                    1 => Some(QReverb::Leger),
                    2 => Some(QReverb::Fort),
                    _ => None,
                };
            }
            WizardStep::Retry => {
                self.retry = match index {
                    0 => Some(QRetry::Premiere),
                    1 => Some(QRetry::PasAssez),
                    2 => Some(QRetry::TropAltere),
                    _ => None,
                };
            }
            WizardStep::Result => {
                self.result = match index {
                    0 => Some(QResult::VoixSeule),
                    1 => Some(QResult::Instrumental),
                    2 => Some(QResult::VoixFond),
                    _ => None,
                };
            }
            WizardStep::Dynamics => {
                self.dynamics = match index {
                    0 => Some(QDynamics::Constant),
                    1 => Some(QDynamics::Variable),
                    2 => Some(QDynamics::Comprime),
                    _ => None,
                };
            }
        }
    }

    fn current_choices(&self) -> Vec<(&str, bool)> {
        match self.step {
            WizardStep::Content => vec![
                (t("wizard.content_dialogue"), self.content == Some(QContent::Dialogue)),
                (t("wizard.content_music"), self.content == Some(QContent::Musique)),
                (t("wizard.content_podcast"), self.content == Some(QContent::Podcast)),
                (t("wizard.content_mixed"), self.content == Some(QContent::Mixte)),
            ],
            WizardStep::Quality => vec![
                (t("wizard.quality_studio"), self.quality == Some(QQuality::Studio)),
                (t("wizard.quality_decent"), self.quality == Some(QQuality::Correct)),
                (t("wizard.quality_noisy"), self.quality == Some(QQuality::Bruyant)),
            ],
            WizardStep::Reverb => vec![
                (t("wizard.reverb_none"), self.reverb == Some(QReverb::Aucun)),
                (t("wizard.reverb_light"), self.reverb == Some(QReverb::Leger)),
                (t("wizard.reverb_heavy"), self.reverb == Some(QReverb::Fort)),
            ],
            WizardStep::Retry => vec![
                (t("wizard.retry_first"), self.retry == Some(QRetry::Premiere)),
                (t("wizard.retry_vocals"), self.retry == Some(QRetry::PasAssez)),
                (t("wizard.retry_altered"), self.retry == Some(QRetry::TropAltere)),
            ],
            WizardStep::Result => vec![
                (t("wizard.result_vocals"), self.result == Some(QResult::VoixSeule)),
                (t("wizard.result_instrumental"), self.result == Some(QResult::Instrumental)),
                (t("wizard.result_mixed"), self.result == Some(QResult::VoixFond)),
            ],
            WizardStep::Dynamics => vec![
                (t("wizard.dynamics_steady"), self.dynamics == Some(QDynamics::Constant)),
                (t("wizard.dynamics_variable"), self.dynamics == Some(QDynamics::Variable)),
                (t("wizard.dynamics_compressed"), self.dynamics == Some(QDynamics::Comprime)),
            ],
        }
    }

    fn step_title(&self) -> &str {
        match self.step {
            WizardStep::Content => t("wizard.title_content"),
            WizardStep::Quality => t("wizard.title_quality"),
            WizardStep::Reverb => t("wizard.title_reverb"),
            WizardStep::Retry => t("wizard.title_retry"),
            WizardStep::Result => t("wizard.title_result"),
            WizardStep::Dynamics => t("wizard.title_dynamics"),
        }
    }

    pub fn render<'a>(&'a self, quads: &mut Vec<QuadInstance>, labels: &mut Vec<LabelInfo<'a>>, sw: f32, sh: f32) {
        let card = Self::card_rect(sw, sh);

        // Dim
        quads.push(QuadInstance {
            rect: [0.0, 0.0, sw, sh],
            color: [0.0, 0.0, 0.0, 0.75], color_bottom: [0.0, 0.0, 0.0, 0.75],
            border_color: [0.0; 4], border_width: 0.0, border_radius: 0.0,
            shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0,
            rotation: 0.0, _padding: [0.0; 2],
        });
        // Card
        quads.push(QuadInstance {
            rect: [card.x, card.y, card.width, card.height],
            color: [0.22, 0.22, 0.26, 1.0], color_bottom: [0.16, 0.16, 0.19, 1.0],
            border_color: [0.40, 0.55, 0.75, 0.8],
            border_width: 1.5, border_radius: 14.0,
            shadow_offset: [0.0, 4.0], shadow_color: [0.0, 0.0, 0.0, 0.5], shadow_blur: 10.0,
            rotation: 0.0, _padding: [0.0; 2],
        });

        // Title
        labels.push(LabelInfo {
            text: t("tools.vocal_remover"),
            bounds: Rect { x: card.x, y: card.y + 8.0, width: card.width, height: 24.0 },
            h_align: HAlign::Center, v_align: VAlign::Center,
            overflow: Overflow::Clip, padding: 0.0,
            font_size_override: Some(16.0), color_override: None, font_family_override: None,
        });

        // Subtitle: question
        labels.push(LabelInfo {
            text: self.step_title(),
            bounds: Rect { x: card.x + 20.0, y: card.y + 34.0, width: card.width - 40.0, height: 18.0 },
            h_align: HAlign::Left, v_align: VAlign::Center,
            overflow: Overflow::Clip, padding: 0.0,
            font_size_override: Some(12.0), color_override: Some([180, 200, 230]), font_family_override: None,
        });

        // Step indicator: "2/6"
        labels.push(LabelInfo {
            text: &self.step_label,
            bounds: Rect { x: card.x + card.width - 60.0, y: card.y + 34.0, width: 40.0, height: 18.0 },
            h_align: HAlign::Right, v_align: VAlign::Center,
            overflow: Overflow::Clip, padding: 0.0,
            font_size_override: Some(10.0), color_override: Some([120, 120, 140]), font_family_override: None,
        });

        // Options
        let base_y = card.y + 56.0;
        let px = card.x + 30.0;
        let choices = self.current_choices();

        for (i, (label, selected)) in choices.iter().enumerate() {
            let ry = base_y + i as f32 * (OPT_H + OPT_GAP);
            let opt_w = CARD_W - 60.0;

            let bg_color = if *selected {
                [0.25, 0.35, 0.50, 1.0]
            } else {
                [0.15, 0.15, 0.18, 1.0]
            };
            let border_color = if *selected {
                [0.50, 0.65, 0.85, 0.9]
            } else {
                [0.30, 0.30, 0.36, 0.5]
            };

            quads.push(QuadInstance {
                rect: [px, ry, opt_w, OPT_H],
                color: bg_color, color_bottom: bg_color,
                border_color, border_width: 1.0, border_radius: 6.0,
                shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0,
                rotation: 0.0, _padding: [0.0; 2],
            });

            let text_color = if *selected { Some([230, 240, 255]) } else { None };
            labels.push(LabelInfo {
                text: label,
                bounds: Rect { x: px + 12.0, y: ry, width: opt_w - 24.0, height: OPT_H },
                h_align: HAlign::Left, v_align: VAlign::Center,
                overflow: Overflow::Clip, padding: 0.0,
                font_size_override: Some(12.0), color_override: text_color, font_family_override: None,
            });
        }

        // Back button
        let btn_y = card.y + CARD_H - 44.0;
        let back_x = card.x + 30.0;
        quads.push(QuadInstance {
            rect: [back_x, btn_y, 100.0, 30.0],
            color: [0.18, 0.18, 0.22, 1.0], color_bottom: [0.18, 0.18, 0.22, 1.0],
            border_color: [0.35, 0.35, 0.42, 0.6], border_width: 1.0, border_radius: 4.0,
            shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0,
            rotation: 0.0, _padding: [0.0; 2],
        });
        labels.push(LabelInfo {
            text: t("wizard.back"),
            bounds: Rect { x: back_x, y: btn_y, width: 100.0, height: 30.0 },
            h_align: HAlign::Center, v_align: VAlign::Center,
            overflow: Overflow::Clip, padding: 0.0,
            font_size_override: Some(11.0), color_override: None, font_family_override: None,
        });

        // Cancel button
        let cancel_x = card.x + CARD_W - 130.0;
        quads.push(QuadInstance {
            rect: [cancel_x, btn_y, 100.0, 30.0],
            color: [0.18, 0.18, 0.22, 1.0], color_bottom: [0.18, 0.18, 0.22, 1.0],
            border_color: [0.35, 0.35, 0.42, 0.6], border_width: 1.0, border_radius: 4.0,
            shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0,
            rotation: 0.0, _padding: [0.0; 2],
        });
        labels.push(LabelInfo {
            text: t("wizard.cancel"),
            bounds: Rect { x: cancel_x, y: btn_y, width: 100.0, height: 30.0 },
            h_align: HAlign::Center, v_align: VAlign::Center,
            overflow: Overflow::Clip, padding: 0.0,
            font_size_override: Some(11.0), color_override: None, font_family_override: None,
        });
    }
}

// --- Preset computation ---
// Indices: 0=room_size, 1=damping, 2=dry, 3=wet, 4=highpass, 5=lowpass,
//          6=comp_threshold, 7=comp_ratio, 8=comp_attack, 9=comp_release, 10=bg_gain

const PARAM_DEFAULTS: [f32; 11] = [
    0.15, 0.7, 0.8, 0.2, 20.0, 22050.0, -15.0, 4.0, 1.0, 100.0, 0.0,
];

const PARAM_MINS: [f32; 11] = [
    0.0, 0.0, 0.0, 0.0, 20.0, 1000.0, -60.0, 1.0, 0.1, 10.0, -6.0,
];

const PARAM_MAXS: [f32; 11] = [
    1.0, 1.0, 1.0, 1.0, 500.0, 22050.0, 0.0, 20.0, 50.0, 1000.0, 6.0,
];

fn apply_delta(params: &mut [f32; 11], idx: usize, delta: f32) {
    params[idx] = (params[idx] + delta).clamp(PARAM_MINS[idx], PARAM_MAXS[idx]);
}

fn set_val(params: &mut [f32; 11], idx: usize, val: f32) {
    params[idx] = val.clamp(PARAM_MINS[idx], PARAM_MAXS[idx]);
}

pub fn compute_preset(wizard: &VocalWizard) -> [f32; 11] {
    let mut p = PARAM_DEFAULTS;

    // Q1: Content type — affects compression and filtering
    match wizard.content {
        Some(QContent::Dialogue) => {
            set_val(&mut p, 6, -30.0);    // threshold: catch quiet bg
            set_val(&mut p, 7, 8.0);      // ratio: aggressive
            set_val(&mut p, 4, 120.0);    // highpass: cut vocal bleed
        }
        Some(QContent::Musique) => {
            set_val(&mut p, 6, -20.0);    // threshold: moderate
            set_val(&mut p, 7, 4.0);      // ratio: moderate
            set_val(&mut p, 5, 16000.0);  // lowpass: preserve highs
        }
        Some(QContent::Podcast) => {
            set_val(&mut p, 6, -40.0);    // threshold: catch everything
            set_val(&mut p, 7, 12.0);     // ratio: aggressive
            set_val(&mut p, 4, 150.0);    // highpass: cut vocal bleed
        }
        Some(QContent::Mixte) => {
            set_val(&mut p, 6, -15.0);    // threshold: default-ish
            set_val(&mut p, 7, 4.0);      // ratio: moderate
        }
        None => {}
    }

    // Q2: Quality — affects filters and reverb on background
    match wizard.quality {
        Some(QQuality::Studio) => {
            set_val(&mut p, 8, 0.5);     // fast attack
            set_val(&mut p, 9, 80.0);    // quick release
            set_val(&mut p, 1, 0.8);     // high damping (clean)
            set_val(&mut p, 3, 0.1);     // low wet (no extra reverb)
        }
        Some(QQuality::Correct) => {
            set_val(&mut p, 8, 1.0);
            set_val(&mut p, 9, 100.0);
            set_val(&mut p, 1, 0.7);
            set_val(&mut p, 3, 0.2);
        }
        Some(QQuality::Bruyant) => {
            set_val(&mut p, 8, 2.0);     // slower attack
            set_val(&mut p, 9, 150.0);   // longer release
            set_val(&mut p, 1, 0.4);     // low damping (preserve clarity)
            set_val(&mut p, 3, 0.35);    // more wet (mask noise)
            set_val(&mut p, 4, 150.0);   // highpass: cut low rumble/noise
        }
        None => {}
    }

    // Q3: Natural reverb
    match wizard.reverb {
        Some(QReverb::Aucun) => {
            set_val(&mut p, 0, 0.05);
            set_val(&mut p, 2, 0.95);   // mostly dry
            set_val(&mut p, 3, 0.05);
            set_val(&mut p, 1, 0.9);    // high damping
        }
        Some(QReverb::Leger) => {
            // Keep defaults (designed for slight reverb)
            set_val(&mut p, 0, 0.15);
            set_val(&mut p, 2, 0.85);
            set_val(&mut p, 3, 0.15);
            set_val(&mut p, 1, 0.7);
        }
        Some(QReverb::Fort) => {
            set_val(&mut p, 0, 0.4);    // large room
            set_val(&mut p, 2, 0.7);    // less dry
            set_val(&mut p, 3, 0.3);    // significant wet
            set_val(&mut p, 1, 0.5);    // moderate damping
        }
        None => {}
    }

    // Q4: Retry context
    match wizard.retry {
        Some(QRetry::Premiere) => { /* no adjustment */ }
        Some(QRetry::PasAssez) => {
            apply_delta(&mut p, 7, 4.0);   // increase ratio
            apply_delta(&mut p, 6, 10.0);  // raise threshold
            set_val(&mut p, 4, 180.0);     // highpass: aggressive cut vocal bleed
            set_val(&mut p, 5, 11000.0);   // lowpass: cut vocal sibilance
        }
        Some(QRetry::TropAltere) => {
            apply_delta(&mut p, 7, -2.0);  // reduce ratio
            set_val(&mut p, 3, 0.1);       // minimal wet
        }
        None => {}
    }

    // Q5: Desired result
    match wizard.result {
        Some(QResult::VoixSeule) => {
            set_val(&mut p, 3, 0.05);       // minimal wet
            set_val(&mut p, 4, 250.0);      // aggressive highpass: kill vocal bleed
        }
        Some(QResult::Instrumental) => {
            set_val(&mut p, 3, 0.3);        // more reverb for fullness
        }
        Some(QResult::VoixFond) => {
            set_val(&mut p, 3, 0.15);        // moderate wet
        }
        None => {}
    }

    // Q6: Dynamics
    match wizard.dynamics {
        Some(QDynamics::Constant) => {
            set_val(&mut p, 6, -10.0);   // moderate threshold
            set_val(&mut p, 7, 3.0);     // light compression
            set_val(&mut p, 8, 1.0);
            set_val(&mut p, 9, 80.0);
        }
        Some(QDynamics::Variable) => {
            set_val(&mut p, 6, -35.0);   // low threshold to catch quiet parts
            set_val(&mut p, 7, 8.0);     // high ratio
            set_val(&mut p, 8, 0.5);     // fast attack
            set_val(&mut p, 9, 120.0);   // moderate release
        }
        Some(QDynamics::Comprime) => {
            set_val(&mut p, 6, 0.0);     // high threshold (already compressed)
            set_val(&mut p, 7, 2.0);     // low ratio (don't over-compress)
            set_val(&mut p, 8, 3.0);     // slower attack
            set_val(&mut p, 9, 200.0);   // long release
        }
        None => {}
    }

    // Clamp all values
    for i in 0..11 {
        p[i] = p[i].clamp(PARAM_MINS[i], PARAM_MAXS[i]);
    }

    p
}