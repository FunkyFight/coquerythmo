from pathlib import Path

project_path = Path("src/project.rs")
project = project_path.read_text(encoding="utf-8")

enum_block = '''#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
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

'''
project = project.replace(enum_block, "", 1)
project = project.replace(
    '    #[serde(default, skip_serializing_if = "is_default_syllable_language")]\n    pub syllable_language: SyllableLanguage,\n',
    "",
    1,
)
project = project.replace(
    'fn is_default_syllable_language(language: &SyllableLanguage) -> bool {\n    *language == SyllableLanguage::default()\n}\n\n',
    "",
    1,
)
project_path.write_text(project, encoding="utf-8")

script_path = Path("scripts/agent_language_patch.py")
script = script_path.read_text(encoding="utf-8")
old = '''    "        let mut settings = ProjectSettings::default();\\n        settings\\n            .export_configuration",
    "        let mut settings = ProjectSettings {\\n            syllable_language: SyllableLanguage::from_code(&language.code),\\n            ..ProjectSettings::default()\\n        };\\n        settings\\n            .export_configuration",
'''
new = '''    "        let language_id = language.id;\\n        let mut settings = ProjectSettings::default();\\n        settings\\n            .export_configuration",
    "        let language_id = language.id;\\n        let mut settings = ProjectSettings {\\n            syllable_language: SyllableLanguage::from_code(&language.code),\\n            ..ProjectSettings::default()\\n        };\\n        settings\\n            .export_configuration",
'''
if old not in script:
    raise RuntimeError("ambiguous constructor replacement not found")
script_path.write_text(script.replace(old, new, 1), encoding="utf-8")
