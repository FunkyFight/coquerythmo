from pathlib import Path
import re


def read(path: str) -> str:
    return Path(path).read_text(encoding="utf-8")


def write(path: str, text: str) -> None:
    Path(path).write_text(text, encoding="utf-8")


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise RuntimeError(f"{label}: expected one occurrence, found {count}")
    return text.replace(old, new, 1)


# Domain ---------------------------------------------------------------------
path = "src/project.rs"
text = read(path)
text = replace_once(
    text,
    "#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]\npub struct ProjectSettings {",
    """#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = \"snake_case\")]
pub enum SyllableLanguage {
    #[default]
    French,
    English,
}

impl SyllableLanguage {
    pub fn code(self) -> &'static str {
        match self {
            Self::French => \"fr-fr\",
            Self::English => \"en-us\",
        }
    }

    pub fn from_code(code: &str) -> Self {
        let normalized = code.trim().to_lowercase();
        if normalized == \"en\"
            || normalized.starts_with(\"en-\")
            || normalized.contains(\"english\")
            || normalized.contains(\"anglais\")
        {
            Self::English
        } else {
            Self::French
        }
    }

    pub fn toggled(self) -> Self {
        match self {
            Self::French => Self::English,
            Self::English => Self::French,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ProjectSettings {""",
    "insert syllable language enum",
)
text = replace_once(
    text,
    "    #[serde(default, skip_serializing_if = \"is_false\")]\n    pub scrolling_text_uses_character_color: bool,\n    #[serde(default, skip_serializing_if = \"is_default_export_configuration\")]",
    "    #[serde(default, skip_serializing_if = \"is_false\")]\n    pub scrolling_text_uses_character_color: bool,\n    #[serde(default, skip_serializing_if = \"is_default_syllable_language\")]\n    pub syllable_language: SyllableLanguage,\n    #[serde(default, skip_serializing_if = \"is_default_export_configuration\")]",
    "add project setting",
)
text = replace_once(
    text,
    "fn is_default_export_configuration(configuration: &ExportConfiguration) -> bool {",
    "fn is_default_syllable_language(language: &SyllableLanguage) -> bool {\n    *language == SyllableLanguage::default()\n}\n\nfn is_default_export_configuration(configuration: &ExportConfiguration) -> bool {",
    "add serde default helper",
)
text = replace_once(
    text,
    "        let language_id = language.id;\n        let mut settings = ProjectSettings::default();\n        settings\n            .export_configuration",
    "        let language_id = language.id;\n        let mut settings = ProjectSettings {\n            syllable_language: SyllableLanguage::from_code(&language.code),\n            ..ProjectSettings::default()\n        };\n        settings\n            .export_configuration",
    "initialize first language",
)
text = replace_once(
    text,
    "    pub fn settings(&self) -> &ProjectSettings {\n        &self.settings\n    }\n\n    pub fn active_language(&self) -> &ProjectLanguage {",
    """    pub fn settings(&self) -> &ProjectSettings {
        &self.settings
    }

    pub fn syllable_language(&self) -> SyllableLanguage {
        self.settings.syllable_language
    }

    pub fn syllable_language_code(&self) -> &'static str {
        self.syllable_language().code()
    }

    pub fn active_language(&self) -> &ProjectLanguage {""",
    "add active syllable accessors",
)
text = replace_once(
    text,
    "        let mut band = self.current_band_snapshot();\n        band.settings.instrumental_audio_path = None;",
    "        let mut band = self.current_band_snapshot();\n        let syllable_language = SyllableLanguage::from_code(&language.code);\n        if band.settings.syllable_language != syllable_language {\n            band.settings.syllable_language = syllable_language;\n            Self::clear_band_syllable_ratios(&mut band.line_map);\n        }\n        band.settings.instrumental_audio_path = None;",
    "initialize duplicated language syllables",
)
insert_after = """    pub fn language_instrumental_audio_path(&self, id: LanguageId) -> Option<String> {
        if id == self.active_language.id {
            return self.settings.instrumental_audio_path.clone();
        }
        self.language_snapshots
            .get(&id)
            .and_then(|snapshot| snapshot.band.settings.instrumental_audio_path.clone())
    }
"""
replacement = insert_after + """
    pub fn language_syllable_language(&self, id: LanguageId) -> Option<SyllableLanguage> {
        if id == self.active_language.id {
            return Some(self.settings.syllable_language);
        }
        self.language_snapshots
            .get(&id)
            .map(|snapshot| snapshot.band.settings.syllable_language)
    }

    pub fn set_language_syllable_language(
        &mut self,
        id: LanguageId,
        language: SyllableLanguage,
    ) -> bool {
        if id == self.active_language.id {
            if self.settings.syllable_language == language {
                return false;
            }
            self.settings.syllable_language = language;
            Self::clear_band_syllable_ratios(&mut self.line_map);
            self.bump_revision();
            return true;
        }

        let Some(snapshot) = self.language_snapshots.get_mut(&id) else {
            return false;
        };
        if snapshot.band.settings.syllable_language == language {
            return false;
        }
        snapshot.band.settings.syllable_language = language;
        Self::clear_band_syllable_ratios(&mut snapshot.band.line_map);
        snapshot.band.revision = snapshot.band.revision.wrapping_add(1);
        self.bump_revision();
        true
    }

    fn clear_band_syllable_ratios(lines: &mut HashMap<u64, RythmoLine>) {
        for line in lines.values_mut() {
            line.syllable_ratios.clear();
        }
    }
"""
text = replace_once(text, insert_after, replacement, "add language syllable domain methods")
test_anchor = """    #[test]
    fn export_configuration_is_shared_across_language_switches() {
"""
new_tests = """    #[test]
    fn syllable_language_is_scoped_to_each_language_band() {
        let mut project = Project::new_with_language(\"Français\", \"fr-fr\");
        let french_id = project.active_language_id();
        let line_id = project.add_line_full(
            0,
            48,
            0.5,
            \"tambourine\".into(),
            \"A\".into(),
            [1.0; 4],
        );
        project.get_line_mut(line_id).unwrap().syllable_ratios = vec![0.25, 0.75];

        let english_id = project.create_language(\"English\", \"en\");
        assert_eq!(project.syllable_language(), SyllableLanguage::English);
        assert!(project.get_line(line_id).unwrap().syllable_ratios.is_empty());

        assert!(project.select_language(french_id));
        assert_eq!(project.syllable_language(), SyllableLanguage::French);
        assert_eq!(
            project.get_line(line_id).unwrap().syllable_ratios,
            vec![0.25, 0.75]
        );
        assert_eq!(
            project.language_syllable_language(english_id),
            Some(SyllableLanguage::English)
        );
    }

    #[test]
    fn changing_syllable_language_resets_only_that_bands_saved_timings() {
        let mut project = Project::new_with_language(\"Français\", \"fr-fr\");
        let french_id = project.active_language_id();
        let line_id = project.add_line_full(0, 48, 0.5, \"Bonjour\".into(), \"A\".into(), [1.0; 4]);
        project.get_line_mut(line_id).unwrap().syllable_ratios = vec![0.4, 0.6];
        let english_id = project.create_language(\"English\", \"en\");
        project.get_line_mut(line_id).unwrap().syllable_ratios = vec![0.2, 0.3, 0.5];

        assert!(project.set_language_syllable_language(english_id, SyllableLanguage::French));
        assert!(project.get_line(line_id).unwrap().syllable_ratios.is_empty());
        assert!(project.select_language(french_id));
        assert_eq!(
            project.get_line(line_id).unwrap().syllable_ratios,
            vec![0.4, 0.6]
        );
    }

""" + test_anchor
text = replace_once(text, test_anchor, new_tests, "add domain tests")
write(path, text)

# Shared render scene ---------------------------------------------------------
path = "src/rendering/rythmo/scene.rs"
text = read(path)
text = replace_once(
    text,
    "pub struct RythmoScene {\n    pub frame_window: FrameWindow,",
    "pub struct RythmoScene {\n    pub frame_window: FrameWindow,\n    pub syllable_language: crate::project::SyllableLanguage,",
    "add scene syllable language",
)
text = replace_once(
    text,
    "        Self {\n            frame_window: options.frame_window,\n            current_frame: options.current_frame,",
    "        Self {\n            frame_window: options.frame_window,\n            syllable_language: project.syllable_language(),\n            current_frame: options.current_frame,",
    "populate scene syllable language",
)
scene_test_anchor = """    #[test]
    fn scene_build_is_deterministic_and_revision_indexed() {
"""
scene_test = """    #[test]
    fn scene_carries_the_projects_syllable_language_for_all_render_backends() {
        let mut project = Project::new_with_language(\"English\", \"en\");
        project.set_language_syllable_language(
            project.active_language_id(),
            crate::project::SyllableLanguage::English,
        );
        let mut render_index = ProjectRenderIndex::new();
        render_index.refresh(&project);

        let scene = RythmoScene::build(&project, &render_index, SceneOptions::default());

        assert_eq!(
            scene.syllable_language,
            crate::project::SyllableLanguage::English
        );
        assert_eq!(scene.syllable_language.code(), \"en-us\");
    }

""" + scene_test_anchor
text = replace_once(text, scene_test_anchor, scene_test, "add scene parity test")
write(path, text)

# Semantic command path -------------------------------------------------------
path = "src/application/command.rs"
text = read(path)
text = replace_once(
    text,
    "    SelectLanguage {\n        id: u64,\n    },\n    PickLanguageInstrumentalAudio {",
    "    SelectLanguage {\n        id: u64,\n    },\n    SetLanguageSyllableLanguage {\n        id: u64,\n        language: crate::project::SyllableLanguage,\n    },\n    PickLanguageInstrumentalAudio {",
    "add semantic action",
)
text = replace_once(
    text,
    "                | Self::SelectLanguage { .. }\n                | Self::PickLanguageInstrumentalAudio { .. }",
    "                | Self::SelectLanguage { .. }\n                | Self::SetLanguageSyllableLanguage { .. }\n                | Self::PickLanguageInstrumentalAudio { .. }",
    "mark semantic action mutating",
)
write(path, text)

path = "src/app/dispatcher.rs"
text = read(path)
text = replace_once(
    text,
    "            UiAction::SelectLanguage { id } => state.select_language(id),\n            UiAction::PickLanguageInstrumentalAudio { id } => {",
    "            UiAction::SelectLanguage { id } => state.select_language(id),\n            UiAction::SetLanguageSyllableLanguage { id, language } => {\n                state.set_language_syllable_language(id, language)\n            }\n            UiAction::PickLanguageInstrumentalAudio { id } => {",
    "dispatch syllable language action",
)
write(path, text)

# State/application service ---------------------------------------------------
path = "src/state.rs"
text = read(path)
text = text.replace(
    "let lang = crate::config::get().lang.clone();",
    "let lang = self.project_session.project.syllable_language_code();",
)
if text.count("crate::config::get().lang") != 0:
    raise RuntimeError("state still uses UI language for syllables")
text = replace_once(
    text,
    "                    let new_ratios = if new_karaoke {\n                        crate::syllable::timing_ratios(&line.text, &line.syllable_ratios, &lang)",
    "                    let new_ratios = if new_karaoke {\n                        crate::syllable::timing_ratios(&line.text, &line.syllable_ratios, lang)",
    "toggle karaoke project language",
)
text = text.replace("                    &lang,\n                    cursor_pos,", "                    lang,\n                    cursor_pos,")
text = text.replace("                    &lang,\n                    progress,", "                    lang,\n                    progress,")
text = replace_once(
    text,
    "                instrumental_audio_path: self\n                    .project_session\n                    .project\n                    .language_instrumental_audio_path(language.id),",
    "                instrumental_audio_path: self\n                    .project_session\n                    .project\n                    .language_instrumental_audio_path(language.id),\n                syllable_language: self\n                    .project_session\n                    .project\n                    .language_syllable_language(language.id)\n                    .unwrap_or_default(),",
    "expose language setting to view model",
)
state_anchor = """    pub fn set_language_instrumental_audio(&mut self, id: u64, path: Option<String>) {
"""
state_method = """    pub fn set_language_syllable_language(
        &mut self,
        id: u64,
        language: crate::project::SyllableLanguage,
    ) {
        let active = id == self.project_session.project.active_language_id();
        if self
            .project_session
            .project
            .set_language_syllable_language(id, language)
        {
            self.project_session.dirty = true;
            if active {
                self.project_session.history.clear();
                self.project_session.render_index = crate::render_index::ProjectRenderIndex::new();
            }
            self.refresh_languages_modal();
        }
    }

""" + state_anchor
text = replace_once(text, state_anchor, state_method, "add state use case")
write(path, text)

# Language modal --------------------------------------------------------------
path = "src/ui/language_modal.rs"
text = read(path)
text = replace_once(
    text,
    "const LIST_BOTTOM: f32 = 70.0;",
    "const LIST_BOTTOM: f32 = 70.0;\nconst CONTROL_COUNT: usize = 10;",
    "add focus count",
)
text = replace_once(
    text,
    "    pub instrumental_audio_path: Option<String>,\n}",
    "    pub instrumental_audio_path: Option<String>,\n    pub syllable_language: crate::project::SyllableLanguage,\n}",
    "add view model value",
)
text = replace_once(
    text,
    "pub enum LanguageModalResult {",
    "#[derive(Debug, PartialEq)]\npub enum LanguageModalResult {",
    "derive result",
)
text = replace_once(
    text,
    "    Select { id: u64 },\n    PickInstrumental { id: u64 },",
    "    Select { id: u64 },\n    SetSyllableLanguage {\n        id: u64,\n        language: crate::project::SyllableLanguage,\n    },\n    PickInstrumental { id: u64 },",
    "add modal result",
)
text = replace_once(
    text,
    "            6 => t(\"languages.delete\").to_string(),\n            7 => t(\"languages.clear_instrumental\").to_string(),\n            _ => t(\"file_explorer.cancel\").to_string(),",
    "            6 => t(\"languages.delete\").to_string(),\n            7 => format!(\n                \"{}: {}\",\n                t(\"languages.syllables\"),\n                self.selected()\n                    .map(|language| syllable_language_label(language.syllable_language))\n                    .unwrap_or(t(\"languages.syllables.french\"))\n            ),\n            8 => t(\"languages.clear_instrumental\").to_string(),\n            _ => t(\"file_explorer.cancel\").to_string(),",
    "accessible focus label",
)
text = replace_once(
    text,
    "    fn sync_name_from_selection(&mut self) {",
    "    pub fn keyboard_focus_role(&self) -> &'static str {\n        match self.keyboard_focus {\n            0 => \"list box\",\n            1 => \"text field\",\n            7 => \"radio group\",\n            _ => \"button\",\n        }\n    }\n\n    fn sync_name_from_selection(&mut self) {",
    "accessible role",
)
text = replace_once(
    text,
    "    fn clear_audio_rect(details: Rect) -> Rect {",
    "    fn syllable_option_rect(details: Rect, index: usize) -> Rect {\n        let gap = 8.0;\n        let width = (details.width - gap) / 2.0;\n        Rect {\n            x: details.x + index.min(1) as f32 * (width + gap),\n            y: details.y + 362.0,\n            width,\n            height: 36.0,\n        }\n    }\n\n    fn clear_audio_rect(details: Rect) -> Rect {",
    "add syllable option geometry",
)
text = text.replace("(self.keyboard_focus + 1) % 9", "(self.keyboard_focus + 1) % CONTROL_COUNT")
text = text.replace("(self.keyboard_focus + 8) % 9", "(self.keyboard_focus + CONTROL_COUNT - 1) % CONTROL_COUNT")
text = replace_once(
    text,
    "                        6 if self.languages.len() > 1 && self.selected().is_some() => {\n                            LanguageModalResult::Delete {\n                                id: self.selected_id,\n                            }\n                        }\n                        7 if self",
    "                        6 if self.languages.len() > 1 && self.selected().is_some() => {\n                            LanguageModalResult::Delete {\n                                id: self.selected_id,\n                            }\n                        }\n                        7 if self.selected().is_some() => LanguageModalResult::SetSyllableLanguage {\n                            id: self.selected_id,\n                            language: self\n                                .selected()\n                                .map(|language| language.syllable_language.toggled())\n                                .unwrap_or_default(),\n                        },\n                        8 if self",
    "keyboard activation",
)
text = text.replace("                        8 => LanguageModalResult::Close,", "                        9 => LanguageModalResult::Close,")
arrow_anchor = """            UiEvent::Scroll { x, y, delta, .. } if list.contains(*x, *y) => {
"""
arrow_branch = """            UiEvent::CursorLeft | UiEvent::CursorRight if self.keyboard_focus == 7 => {
                let language = if matches!(event, UiEvent::CursorRight) {
                    crate::project::SyllableLanguage::English
                } else {
                    crate::project::SyllableLanguage::French
                };
                if self.selected().is_some_and(|selected| selected.syllable_language != language) {
                    LanguageModalResult::SetSyllableLanguage {
                        id: self.selected_id,
                        language,
                    }
                } else {
                    LanguageModalResult::Consumed
                }
            }
""" + arrow_anchor
text = replace_once(text, arrow_anchor, arrow_branch, "keyboard arrows")
mouse_anchor = """                } else if Self::clear_audio_rect(details).contains(*x, *y)
"""
mouse_choice = """                } else if Self::syllable_option_rect(details, 0).contains(*x, *y) {
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
"""
text = replace_once(text, mouse_anchor, mouse_choice, "pointer choice")
render_anchor = """        let audio_path = self
"""
render_choice = """        labels.push(label(
            t(\"languages.syllables\"),
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
"""
text = replace_once(text, render_anchor, render_choice, "render syllable choices")
text = replace_once(
    text,
    "fn push_action<'a>(",
    "pub(super) fn syllable_language_label(\n    language: crate::project::SyllableLanguage,\n) -> &'static str {\n    match language {\n        crate::project::SyllableLanguage::French => t(\"languages.syllables.french\"),\n        crate::project::SyllableLanguage::English => t(\"languages.syllables.english\"),\n    }\n}\n\nfn push_action<'a>(",
    "add localized value helper",
)
old_test = """    #[test]
    fn instrumental_details_start_below_last_language_action() {
"""
new_test = """    fn item(language: crate::project::SyllableLanguage) -> LanguageListItem {
        LanguageListItem {
            id: 1,
            name: \"English\".into(),
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
                &UiEvent::KeyInput { text: \"\\r\".into() },
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
    fn tab_and_shift_tab_traverse_the_syllable_control_deterministically() {
        let mut modal = LanguageModal::new(
            vec![item(crate::project::SyllableLanguage::French)],
            1,
        );
        modal.handle_event(
            &UiEvent::KeyInput { text: \"\\u{b}\".into() },
            1280.0,
            720.0,
        );
        assert_eq!(modal.keyboard_focus, CONTROL_COUNT - 1);
        for _ in 0..8 {
            modal.handle_event(
                &UiEvent::KeyInput { text: \"\\t\".into() },
                1280.0,
                720.0,
            );
        }
        assert_eq!(modal.keyboard_focus, 7);
        assert_eq!(modal.keyboard_focus_role(), \"radio group\");
        assert!(modal.keyboard_focus_label().contains(t(\"languages.syllables\")));
    }

    #[test]
    fn instrumental_details_start_below_last_language_action() {
"""
text = replace_once(text, old_test, new_test, "add modal interaction tests")
text = replace_once(
    text,
    "        assert!(instrumental_label_y >= delete_button.y + delete_button.height + 24.0);",
    "        let syllable_options_bottom = LanguageModal::syllable_option_rect(details, 0).y + 36.0;\n        assert!(syllable_options_bottom + 18.0 <= instrumental_label_y);\n        assert!(instrumental_label_y >= delete_button.y + delete_button.height + 24.0);",
    "extend geometry test",
)
write(path, text)

# Modal host accessibility ----------------------------------------------------
path = "src/ui/modal_host.rs"
text = read(path)
text = replace_once(
    text,
    "                        role: \"control\".to_string(),",
    "                        role: modal.keyboard_focus_role().to_string(),",
    "language modal role",
)
text = replace_once(
    text,
    "            LanguageModalResult::Select { id } => {\n                ModalOutcome::Action(UiAction::SelectLanguage { id })\n            }\n            LanguageModalResult::PickInstrumental { id } => {",
    "            LanguageModalResult::Select { id } => {\n                ModalOutcome::Action(UiAction::SelectLanguage { id })\n            }\n            LanguageModalResult::SetSyllableLanguage { id, language } => {\n                ModalOutcome::Actions(vec![\n                    UiAction::SetLanguageSyllableLanguage { id, language },\n                    UiAction::Accessibility(\n                        crate::accessibility::AccessibilityEvent::ValueChanged {\n                            label: crate::i18n::t(\"languages.syllables\").to_string(),\n                            value: super::language_modal::syllable_language_label(language)\n                                .to_string(),\n                        },\n                    ),\n                ])\n            }\n            LanguageModalResult::PickInstrumental { id } => {",
    "map modal result and announce value",
)
write(path, text)

# Rendering and editing callers ----------------------------------------------
for path in [
    "src/workspaces/rythmo/mouse.rs",
    "src/workspaces/rythmo/mouse_buttons.rs",
    "src/workspaces/rythmo/press.rs",
]:
    text = read(path)
    text = text.replace(
        "let lang = crate::config::get().lang.clone();",
        "let lang = ctx.project.syllable_language_code();",
    )
    text = text.replace("&lang,", "lang,")
    if "crate::config::get().lang" in text:
        raise RuntimeError(f"{path}: UI language caller remains")
    write(path, text)

path = "src/workspaces/rythmo/syllable.rs"
text = read(path)
text = replace_once(
    text,
    "    let lang = crate::config::get().lang.clone();\n    let ratios = syllable_ratios_for_line(line, state.syllable_drag.as_ref(), &lang, state)?;",
    "    let lang = ctx.project.syllable_language_code();\n    let ratios = syllable_ratios_for_line(line, state.syllable_drag.as_ref(), lang, state)?;",
    "syllable drag language",
)
write(path, text)

path = "src/ui/mod.rs"
text = read(path)
text = replace_once(
    text,
    "                    let lang = crate::config::get().lang.clone();",
    "                    let lang = project.syllable_language_code();",
    "cursor render language",
)
text = text.replace("                        &lang,", "                        lang,")
write(path, text)

path = "src/workspaces/rythmo/view.rs"
text = read(path)
text = replace_once(
    text,
    "    let karaoke_lang = crate::config::get().lang.clone();",
    "    let karaoke_lang = project.syllable_language_code();",
    "view render language",
)
text = text.replace("&karaoke_lang", "karaoke_lang")
write(path, text)

path = "src/rythmo_cpu_renderer.rs"
text = read(path)
text = text.replace("&crate::config::get().lang", "scene.syllable_language.code()")
text = replace_once(
    text,
    "                    let lang = scene.syllable_language.code();\n                    let breaks = crate::syllable::syllable_breaks(&line.text, lang);",
    "                    let lang = scene.syllable_language.code();\n                    let breaks = crate::syllable::syllable_breaks(&line.text, lang);",
    "confirm CPU local language",
)
text = replace_once(
    text,
    "                blit_karaoke_dot(&mut pixmap, line, current_frame as f64, x1, line_y, lw, s);",
    "                blit_karaoke_dot(\n                    &mut pixmap,\n                    line,\n                    scene.syllable_language.code(),\n                    current_frame as f64,\n                    x1,\n                    line_y,\n                    lw,\n                    s,\n                );",
    "pass CPU scene language",
)
text = replace_once(
    text,
    "fn blit_karaoke_dot(\n    pixmap: &mut Pixmap,\n    line: &crate::rythmo_line::RythmoLine,\n    current_frame: f64,",
    "fn blit_karaoke_dot(\n    pixmap: &mut Pixmap,\n    line: &crate::rythmo_line::RythmoLine,\n    lang: &str,\n    current_frame: f64,",
    "CPU dot signature",
)
text = text.replace("scene.syllable_language.code(),\n    );\n    let local_progress", "lang,\n    );\n    let local_progress", 1)
text = text.replace("scene.syllable_language.code(),\n        progress,\n    );\n    let bounce", "lang,\n        progress,\n    );\n    let bounce", 1)
if "crate::config::get().lang" in text:
    raise RuntimeError("CPU renderer still uses UI language")
write(path, text)

path = "src/rythmo_gpu_renderer.rs"
text = read(path)
text = text.replace("&crate::config::get().lang", "scene.syllable_language.code()")
text = replace_once(
    text,
    "fn push_karaoke_dot(\n    quads: &mut Vec<QuadInstance>,\n    line: &RythmoLine,\n    current_frame: f64,",
    "fn push_karaoke_dot(\n    quads: &mut Vec<QuadInstance>,\n    line: &RythmoLine,\n    lang: &str,\n    current_frame: f64,",
    "GPU dot signature",
)
text = text.replace("scene.syllable_language.code(),\n    );\n    let local_progress", "lang,\n    );\n    let local_progress", 1)
text = text.replace("scene.syllable_language.code(),\n        progress,\n    );\n    let bounce", "lang,\n        progress,\n    );\n    let bounce", 1)
text = replace_once(
    text,
    "                push_karaoke_dot(&mut quads, line, current_frame, x1, line_y, lw, s);",
    "                push_karaoke_dot(\n                    &mut quads,\n                    line,\n                    scene.syllable_language.code(),\n                    current_frame,\n                    x1,\n                    line_y,\n                    lw,\n                    s,\n                );",
    "pass GPU scene language",
)
if "crate::config::get().lang" in text:
    raise RuntimeError("GPU renderer still uses UI language")
write(path, text)

# i18n -----------------------------------------------------------------------
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

# Final guard: only bootstrap may depend on the UI locale itself.
remaining = []
for source in Path("src").rglob("*.rs"):
    for line_no, line in enumerate(source.read_text(encoding="utf-8").splitlines(), 1):
        if "config::get().lang" in line and source.as_posix() != "src/app/bootstrap.rs":
            remaining.append(f"{source}:{line_no}:{line}")
if remaining:
    raise RuntimeError("UI language still drives project behavior:\n" + "\n".join(remaining))
