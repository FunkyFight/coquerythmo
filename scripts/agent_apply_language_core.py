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


# Project domain --------------------------------------------------------------
path = "src/project.rs"
text = read(path)
text = replace_once(
    text,
    "#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]\npub struct ProjectSettings {",
    '''#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyllableLanguage {
    #[default]
    French,
    English,
}

impl SyllableLanguage {
    pub fn code(self) -> &'static str {
        match self {
            Self::French => "fr-fr",
            Self::English => "en-us",
        }
    }

    pub fn from_code(code: &str) -> Self {
        let normalized = code.trim().to_lowercase();
        if normalized == "en"
            || normalized.starts_with("en-")
            || normalized.contains("english")
            || normalized.contains("anglais")
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
pub struct ProjectSettings {''',
    "insert syllable language enum",
)
text = replace_once(
    text,
    '''    #[serde(default, skip_serializing_if = "is_false")]
    pub scrolling_text_uses_character_color: bool,
    #[serde(default, skip_serializing_if = "is_default_export_configuration")]''',
    '''    #[serde(default, skip_serializing_if = "is_false")]
    pub scrolling_text_uses_character_color: bool,
    #[serde(default, skip_serializing_if = "is_default_syllable_language")]
    pub syllable_language: SyllableLanguage,
    #[serde(default, skip_serializing_if = "is_default_export_configuration")]''',
    "add per-band setting",
)
text = replace_once(
    text,
    "fn is_default_export_configuration(configuration: &ExportConfiguration) -> bool {",
    '''fn is_default_syllable_language(language: &SyllableLanguage) -> bool {
    *language == SyllableLanguage::default()
}

fn is_default_export_configuration(configuration: &ExportConfiguration) -> bool {''',
    "add serde helper",
)
text = replace_once(
    text,
    '''        let language_id = language.id;
        let mut settings = ProjectSettings::default();
        settings
            .export_configuration''',
    '''        let language_id = language.id;
        let mut settings = ProjectSettings {
            syllable_language: SyllableLanguage::from_code(&language.code),
            ..ProjectSettings::default()
        };
        settings
            .export_configuration''',
    "initialize first language",
)
text = replace_once(
    text,
    '''    pub fn settings(&self) -> &ProjectSettings {
        &self.settings
    }

    pub fn active_language(&self) -> &ProjectLanguage {''',
    '''    pub fn settings(&self) -> &ProjectSettings {
        &self.settings
    }

    pub fn syllable_language(&self) -> SyllableLanguage {
        self.settings.syllable_language
    }

    pub fn syllable_language_code(&self) -> &'static str {
        self.syllable_language().code()
    }

    pub fn active_language(&self) -> &ProjectLanguage {''',
    "add active language accessors",
)
text = replace_once(
    text,
    '''        let mut band = self.current_band_snapshot();
        band.settings.instrumental_audio_path = None;''',
    '''        let mut band = self.current_band_snapshot();
        let syllable_language = SyllableLanguage::from_code(&language.code);
        if band.settings.syllable_language != syllable_language {
            band.settings.syllable_language = syllable_language;
            Self::clear_band_syllable_ratios(&mut band.line_map);
        }
        band.settings.instrumental_audio_path = None;''',
    "initialize duplicated language",
)
anchor = '''    pub fn language_instrumental_audio_path(&self, id: LanguageId) -> Option<String> {
        if id == self.active_language.id {
            return self.settings.instrumental_audio_path.clone();
        }
        self.language_snapshots
            .get(&id)
            .and_then(|snapshot| snapshot.band.settings.instrumental_audio_path.clone())
    }
'''
text = replace_once(
    text,
    anchor,
    anchor
    + '''
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
''',
    "add language setting domain API",
)
test_anchor = '''    #[test]
    fn export_configuration_is_shared_across_language_switches() {
'''
text = replace_once(
    text,
    test_anchor,
    '''    #[test]
    fn syllable_language_is_scoped_to_each_language_band() {
        let mut project = Project::new_with_language("Français", "fr-fr");
        let french_id = project.active_language_id();
        let line_id = project.add_line_full(
            0,
            48,
            0.5,
            "tambourine".into(),
            "A".into(),
            [1.0; 4],
        );
        project.get_line_mut(line_id).unwrap().syllable_ratios = vec![0.25, 0.75];

        let english_id = project.create_language("English", "en");
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
    fn changing_syllable_language_resets_only_the_target_band() {
        let mut project = Project::new_with_language("Français", "fr-fr");
        let french_id = project.active_language_id();
        let line_id = project.add_line_full(
            0,
            48,
            0.5,
            "Bonjour".into(),
            "A".into(),
            [1.0; 4],
        );
        project.get_line_mut(line_id).unwrap().syllable_ratios = vec![0.4, 0.6];
        let english_id = project.create_language("English", "en");
        project.get_line_mut(line_id).unwrap().syllable_ratios = vec![0.2, 0.3, 0.5];

        assert!(project.set_language_syllable_language(
            english_id,
            SyllableLanguage::French
        ));
        assert!(project.get_line(line_id).unwrap().syllable_ratios.is_empty());

        assert!(project.select_language(french_id));
        assert_eq!(
            project.get_line(line_id).unwrap().syllable_ratios,
            vec![0.4, 0.6]
        );
    }

'''
    + test_anchor,
    "add domain tests",
)
write(path, text)

# Backend-independent scene ---------------------------------------------------
path = "src/rendering/rythmo/scene.rs"
text = read(path)
text = replace_once(
    text,
    "pub struct RythmoScene {\n    pub frame_window: FrameWindow,",
    "pub struct RythmoScene {\n    pub frame_window: FrameWindow,\n    pub syllable_language: crate::project::SyllableLanguage,",
    "scene field",
)
text = replace_once(
    text,
    '''        Self {
            frame_window: options.frame_window,
            current_frame: options.current_frame,''',
    '''        Self {
            frame_window: options.frame_window,
            syllable_language: project.syllable_language(),
            current_frame: options.current_frame,''',
    "scene population",
)
scene_test_anchor = '''    #[test]
    fn scene_build_is_deterministic_and_revision_indexed() {
'''
text = replace_once(
    text,
    scene_test_anchor,
    '''    #[test]
    fn scene_carries_project_syllable_language_for_cpu_and_gpu_renderers() {
        let project = Project::new_with_language("English", "en");
        let render_index = ProjectRenderIndex::new();

        let scene = RythmoScene::build(&project, &render_index, SceneOptions::default());

        assert_eq!(
            scene.syllable_language,
            crate::project::SyllableLanguage::English
        );
        assert_eq!(scene.syllable_language.code(), "en-us");
    }

'''
    + scene_test_anchor,
    "scene parity test",
)
write(path, text)

# Semantic action and dispatcher ---------------------------------------------
path = "src/application/command.rs"
text = read(path)
text = replace_once(
    text,
    '''    SelectLanguage {
        id: u64,
    },
    PickLanguageInstrumentalAudio {''',
    '''    SelectLanguage {
        id: u64,
    },
    SetLanguageSyllableLanguage {
        id: u64,
        language: crate::project::SyllableLanguage,
    },
    PickLanguageInstrumentalAudio {''',
    "semantic action",
)
text = replace_once(
    text,
    '''                | Self::SelectLanguage { .. }
                | Self::PickLanguageInstrumentalAudio { .. }''',
    '''                | Self::SelectLanguage { .. }
                | Self::SetLanguageSyllableLanguage { .. }
                | Self::PickLanguageInstrumentalAudio { .. }''',
    "mutating action classification",
)
write(path, text)

path = "src/app/dispatcher.rs"
text = read(path)
text = replace_once(
    text,
    '''            UiAction::SelectLanguage { id } => state.select_language(id),
            UiAction::PickLanguageInstrumentalAudio { id } => {''',
    '''            UiAction::SelectLanguage { id } => state.select_language(id),
            UiAction::SetLanguageSyllableLanguage { id, language } => {
                state.set_language_syllable_language(id, language)
            }
            UiAction::PickLanguageInstrumentalAudio { id } => {''',
    "dispatcher path",
)
write(path, text)

# Application service/state ---------------------------------------------------
path = "src/state.rs"
text = read(path)
locale_line = "        let lang = crate::config::get().lang.clone();"
if text.count(locale_line) != 2:
    raise RuntimeError(f"state locale callers: expected 2, found {text.count(locale_line)}")
text = text.replace(
    locale_line,
    "        let lang = self.project_session.project.syllable_language_code();",
)
text = replace_once(
    text,
    "                        crate::syllable::timing_ratios(&line.text, &line.syllable_ratios, &lang)",
    "                        crate::syllable::timing_ratios(&line.text, &line.syllable_ratios, lang)",
    "karaoke toggle caller",
)
text = replace_once(
    text,
    '''                    &lang,
                    cursor_pos,''',
    '''                    lang,
                    cursor_pos,''',
    "dialogue cursor split caller",
)
text = replace_once(
    text,
    '''                    &lang,
                    progress,''',
    '''                    lang,
                    progress,''',
    "dialogue playhead split caller",
)
text = replace_once(
    text,
    '''                instrumental_audio_path: self
                    .project_session
                    .project
                    .language_instrumental_audio_path(language.id),''',
    '''                instrumental_audio_path: self
                    .project_session
                    .project
                    .language_instrumental_audio_path(language.id),
                syllable_language: self
                    .project_session
                    .project
                    .language_syllable_language(language.id)
                    .unwrap_or_default(),''',
    "language panel view model",
)
state_anchor = '''    pub fn set_language_instrumental_audio(&mut self, id: u64, path: Option<String>) {
'''
text = replace_once(
    text,
    state_anchor,
    '''    pub fn set_language_syllable_language(
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

'''
    + state_anchor,
    "state use case",
)
write(path, text)
