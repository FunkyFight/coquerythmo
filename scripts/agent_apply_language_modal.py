from pathlib import Path


def read(path: str) -> str:
    return Path(path).read_text(encoding="utf-8")


def write(path: str, text: str) -> None:
    Path(path).write_text(text, encoding="utf-8")


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise RuntimeError(f"{label}: expected one occurrence, found {count}")
    return text.replace(old, new, 1)


# Language panel --------------------------------------------------------------
path = "src/ui/language_modal.rs"
text = read(path)
text = replace_once(
    text,
    "const LIST_BOTTOM: f32 = 70.0;",
    "const LIST_BOTTOM: f32 = 70.0;\nconst CONTROL_COUNT: usize = 10;",
    "focus count",
)
text = replace_once(
    text,
    '''pub struct LanguageListItem {
    pub id: u64,
    pub name: String,
    pub instrumental_audio_path: Option<String>,
}''',
    '''pub struct LanguageListItem {
    pub id: u64,
    pub name: String,
    pub instrumental_audio_path: Option<String>,
    pub syllable_language: crate::project::SyllableLanguage,
}''',
    "view model value",
)
text = replace_once(
    text,
    "pub enum LanguageModalResult {",
    "#[derive(Debug, PartialEq)]\npub enum LanguageModalResult {",
    "result equality",
)
text = replace_once(
    text,
    '''    Select { id: u64 },
    PickInstrumental { id: u64 },''',
    '''    Select { id: u64 },
    SetSyllableLanguage {
        id: u64,
        language: crate::project::SyllableLanguage,
    },
    PickInstrumental { id: u64 },''',
    "semantic modal result",
)
text = replace_once(
    text,
    '''            5 => t("languages.instrumental").to_string(),
            6 => t("languages.delete").to_string(),
            7 => t("languages.clear_instrumental").to_string(),
            _ => t("file_explorer.cancel").to_string(),''',
    '''            5 => t("languages.instrumental").to_string(),
            6 => t("languages.delete").to_string(),
            7 => format!(
                "{}: {}",
                t("languages.syllables"),
                self.selected()
                    .map(|language| syllable_language_label(language.syllable_language))
                    .unwrap_or(t("languages.syllables.french"))
            ),
            8 => t("languages.clear_instrumental").to_string(),
            _ => t("file_explorer.cancel").to_string(),''',
    "accessible focus value",
)
text = replace_once(
    text,
    "    fn sync_name_from_selection(&mut self) {",
    '''    pub fn keyboard_focus_role(&self) -> &'static str {
        match self.keyboard_focus {
            0 => "list box",
            1 => "text field",
            7 => "radio group",
            _ => "button",
        }
    }

    fn sync_name_from_selection(&mut self) {''',
    "accessible role",
)
text = replace_once(
    text,
    '''    fn clear_audio_rect(details: Rect) -> Rect {
''',
    '''    fn syllable_option_rect(details: Rect, index: usize) -> Rect {
        let gap = 8.0;
        let width = (details.width - gap) / 2.0;
        Rect {
            x: details.x + index.min(1) as f32 * (width + gap),
            y: details.y + 362.0,
            width,
            height: 36.0,
        }
    }

    fn clear_audio_rect(details: Rect) -> Rect {
''',
    "control geometry",
)
text = replace_once(
    text,
    '''                        (self.keyboard_focus + 1) % 9
                    } else {
                        (self.keyboard_focus + 8) % 9''',
    '''                        (self.keyboard_focus + 1) % CONTROL_COUNT
                    } else {
                        (self.keyboard_focus + CONTROL_COUNT - 1) % CONTROL_COUNT''',
    "deterministic focus traversal",
)
text = replace_once(
    text,
    '''                        6 if self.languages.len() > 1 && self.selected().is_some() => {
                            LanguageModalResult::Delete {
                                id: self.selected_id,
                            }
                        }
                        7 if self
                            .selected()
                            .and_then(|language| language.instrumental_audio_path.as_ref())
                            .is_some() =>
                        {
                            LanguageModalResult::ClearInstrumental {
                                id: self.selected_id,
                            }
                        }
                        8 => LanguageModalResult::Close,''',
    '''                        6 if self.languages.len() > 1 && self.selected().is_some() => {
                            LanguageModalResult::Delete {
                                id: self.selected_id,
                            }
                        }
                        7 if self.selected().is_some() => {
                            LanguageModalResult::SetSyllableLanguage {
                                id: self.selected_id,
                                language: self
                                    .selected()
                                    .map(|language| language.syllable_language.toggled())
                                    .unwrap_or_default(),
                            }
                        }
                        8 if self
                            .selected()
                            .and_then(|language| language.instrumental_audio_path.as_ref())
                            .is_some() =>
                        {
                            LanguageModalResult::ClearInstrumental {
                                id: self.selected_id,
                            }
                        }
                        9 => LanguageModalResult::Close,''',
    "keyboard activation",
)
scroll_anchor = '''            UiEvent::Scroll { x, y, delta, .. } if list.contains(*x, *y) => {
'''
text = replace_once(
    text,
    scroll_anchor,
    '''            UiEvent::CursorLeft | UiEvent::CursorRight if self.keyboard_focus == 7 => {
                let language = if matches!(event, UiEvent::CursorRight) {
                    crate::project::SyllableLanguage::English
                } else {
                    crate::project::SyllableLanguage::French
                };
                if self
                    .selected()
                    .is_some_and(|selected| selected.syllable_language != language)
                {
                    LanguageModalResult::SetSyllableLanguage {
                        id: self.selected_id,
                        language,
                    }
                } else {
                    LanguageModalResult::Consumed
                }
            }
'''
    + scroll_anchor,
    "arrow selection",
)
text = replace_once(
    text,
    '''                } else if Self::clear_audio_rect(details).contains(*x, *y)
                    && self
''',
    '''                } else if Self::syllable_option_rect(details, 0).contains(*x, *y) {
                    self.keyboard_focus = 7;
                    if self.selected().is_some_and(|selected| {
                        selected.syllable_language != crate::project::SyllableLanguage::French
                    }) {
                        return LanguageModalResult::SetSyllableLanguage {
                            id: self.selected_id,
                            language: crate::project::SyllableLanguage::French,
                        };
                    }
                } else if Self::syllable_option_rect(details, 1).contains(*x, *y) {
                    self.keyboard_focus = 7;
                    if self.selected().is_some_and(|selected| {
                        selected.syllable_language != crate::project::SyllableLanguage::English
                    }) {
                        return LanguageModalResult::SetSyllableLanguage {
                            id: self.selected_id,
                            language: crate::project::SyllableLanguage::English,
                        };
                    }
                } else if Self::clear_audio_rect(details).contains(*x, *y)
                    && self
''',
    "pointer activation",
)
text = replace_once(
    text,
    '''        let audio_path = self
            .selected()
''',
    '''        labels.push(label(
            t("languages.syllables"),
            Rect {
                x: details.x,
                y: details.y + 338.0,
                width: details.width,
                height: 18.0,
            },
            12.0,
            HAlign::Left,
            Some([137, 143, 160]),
        ));
        let selected_syllable_language = self
            .selected()
            .map(|language| language.syllable_language)
            .unwrap_or_default();
        for (index, language) in [
            crate::project::SyllableLanguage::French,
            crate::project::SyllableLanguage::English,
        ]
        .into_iter()
        .enumerate()
        {
            let selected = language == selected_syllable_language;
            let focused = self.keyboard_focus == 7;
            let rect = Self::syllable_option_rect(details, index);
            push_panel(
                quads,
                rect,
                if selected {
                    [0.16, 0.31, 0.55, 1.0]
                } else {
                    [0.10, 0.11, 0.15, 1.0]
                },
                8.0,
                if selected || focused {
                    [0.31, 0.52, 0.90, 1.0]
                } else {
                    [0.22, 0.25, 0.32, 1.0]
                },
            );
            labels.push(label(
                syllable_language_label(language),
                rect,
                13.0,
                HAlign::Center,
                None,
            ));
        }

        let audio_path = self
            .selected()
''',
    "render control",
)
text = replace_once(
    text,
    "fn push_action<'a>(",
    '''pub(super) fn syllable_language_label(
    language: crate::project::SyllableLanguage,
) -> &'static str {
    match language {
        crate::project::SyllableLanguage::French => t("languages.syllables.french"),
        crate::project::SyllableLanguage::English => t("languages.syllables.english"),
    }
}

fn push_action<'a>(''',
    "localized value helper",
)
old_tests = '''    #[test]
    fn instrumental_details_start_below_last_language_action() {
'''
text = replace_once(
    text,
    old_tests,
    '''    fn item(language: crate::project::SyllableLanguage) -> LanguageListItem {
        LanguageListItem {
            id: 1,
            name: "English".into(),
            instrumental_audio_path: None,
            syllable_language: language,
        }
    }

    #[test]
    fn syllable_language_control_has_keyboard_and_pointer_parity() {
        let mut modal = LanguageModal::new(
            vec![item(crate::project::SyllableLanguage::French)],
            1,
        );
        modal.keyboard_focus = 7;
        assert_eq!(
            modal.handle_event(
                &UiEvent::KeyInput { text: "\r".into() },
                1280.0,
                720.0,
            ),
            LanguageModalResult::SetSyllableLanguage {
                id: 1,
                language: crate::project::SyllableLanguage::English,
            }
        );

        modal.refresh(
            vec![item(crate::project::SyllableLanguage::English)],
            1,
        );
        let details = LanguageModal::details_rect(LanguageModal::card(1280.0, 720.0));
        let french = LanguageModal::syllable_option_rect(details, 0);
        assert_eq!(
            modal.handle_event(
                &UiEvent::MousePress {
                    x: french.x + 2.0,
                    y: french.y + 2.0,
                },
                1280.0,
                720.0,
            ),
            LanguageModalResult::SetSyllableLanguage {
                id: 1,
                language: crate::project::SyllableLanguage::French,
            }
        );
        assert_eq!(modal.keyboard_focus, 7);
    }

    #[test]
    fn tab_and_shift_tab_reach_the_syllable_control_deterministically() {
        let mut modal = LanguageModal::new(
            vec![item(crate::project::SyllableLanguage::French)],
            1,
        );
        modal.handle_event(
            &UiEvent::KeyInput { text: "\\u{b}".into() },
            1280.0,
            720.0,
        );
        assert_eq!(modal.keyboard_focus, CONTROL_COUNT - 1);
        for _ in 0..8 {
            modal.handle_event(
                &UiEvent::KeyInput { text: "\t".into() },
                1280.0,
                720.0,
            );
        }
        assert_eq!(modal.keyboard_focus, 7);
        assert_eq!(modal.keyboard_focus_role(), "radio group");
        assert!(modal.keyboard_focus_label().contains(t("languages.syllables")));
    }

    #[test]
    fn instrumental_details_start_below_last_language_action() {
''',
    "interaction tests",
)
text = replace_once(
    text,
    "        assert!(instrumental_label_y >= delete_button.y + delete_button.height + 24.0);",
    '''        let syllable_options_bottom = LanguageModal::syllable_option_rect(details, 0).y + 36.0;
        assert!(syllable_options_bottom + 18.0 <= instrumental_label_y);
        assert!(instrumental_label_y >= delete_button.y + delete_button.height + 24.0);''',
    "geometry coverage",
)
write(path, text)

# Modal host maps the view result to one semantic command and one accessibility
# value announcement. It also reports the real role during focus traversal.
path = "src/ui/modal_host.rs"
text = read(path)
text = replace_once(
    text,
    '''            if let Some(label) = self
                .languages
                .as_ref()
                .map(|modal| modal.keyboard_focus_label())
            {
                return ModalOutcome::Action(UiAction::Accessibility(
                    crate::accessibility::AccessibilityEvent::Focus {
                        label,
                        role: "control".to_string(),
                    },
                ));
            }
''',
    '''            if let Some((label, role)) = self.languages.as_ref().map(|modal| {
                (
                    modal.keyboard_focus_label(),
                    modal.keyboard_focus_role().to_string(),
                )
            }) {
                return ModalOutcome::Action(UiAction::Accessibility(
                    crate::accessibility::AccessibilityEvent::Focus { label, role },
                ));
            }
''',
    "modal focus role",
)
text = replace_once(
    text,
    '''            LanguageModalResult::Select { id } => {
                ModalOutcome::Action(UiAction::SelectLanguage { id })
            }
            LanguageModalResult::PickInstrumental { id } => {
''',
    '''            LanguageModalResult::Select { id } => {
                ModalOutcome::Action(UiAction::SelectLanguage { id })
            }
            LanguageModalResult::SetSyllableLanguage { id, language } => {
                ModalOutcome::Actions(vec![
                    UiAction::SetLanguageSyllableLanguage { id, language },
                    UiAction::Accessibility(
                        crate::accessibility::AccessibilityEvent::ValueChanged {
                            label: crate::i18n::t("languages.syllables").to_string(),
                            value: super::language_modal::syllable_language_label(language)
                                .to_string(),
                        },
                    ),
                ])
            }
            LanguageModalResult::PickInstrumental { id } => {
''',
    "modal semantic result",
)
write(path, text)

# Localized accessible names and values --------------------------------------
translations = {
    "i18n/fr.toml": (
        '"languages.clear_instrumental" = "Retirer l’instrumental"',
        '"languages.clear_instrumental" = "Retirer l’instrumental"\n"languages.syllables" = "Langue de découpe des syllabes"\n"languages.syllables.french" = "Français"\n"languages.syllables.english" = "Anglais"',
    ),
    "i18n/en.toml": (
        '"languages.clear_instrumental" = "Remove instrumental"',
        '"languages.clear_instrumental" = "Remove instrumental"\n"languages.syllables" = "Syllable language"\n"languages.syllables.french" = "French"\n"languages.syllables.english" = "English"',
    ),
    "i18n/es.toml": (
        '"languages.clear_instrumental" = "Quitar instrumental"',
        '"languages.clear_instrumental" = "Quitar instrumental"\n"languages.syllables" = "Idioma de separación silábica"\n"languages.syllables.french" = "Francés"\n"languages.syllables.english" = "Inglés"',
    ),
}
for path, (old, new) in translations.items():
    text = read(path)
    text = replace_once(text, old, new, f"translations in {path}")
    write(path, text)
