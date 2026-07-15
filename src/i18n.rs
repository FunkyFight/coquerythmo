use std::collections::HashMap;
use std::sync::OnceLock;

static INSTANCE: OnceLock<I18n> = OnceLock::new();

// Embed translation files at compile time — no runtime file dependency.
const FR_TOML: &str = include_str!("../i18n/fr.toml");
const EN_TOML: &str = include_str!("../i18n/en.toml");
const ES_TOML: &str = include_str!("../i18n/es.toml");

struct I18n {
    translations: HashMap<String, String>,
    fallback: HashMap<String, String>,
}

fn load_toml(source: &str) -> HashMap<String, String> {
    match source.parse::<toml::Table>() {
        Ok(table) => table
            .into_iter()
            .filter_map(|(k, v)| v.as_str().map(|s| (k, s.to_string())))
            .collect(),
        Err(e) => {
            log::error!("Failed to parse translations TOML: {e}");
            HashMap::new()
        }
    }
}

impl I18n {
    fn new(lang: &str) -> Self {
        let primary = match lang {
            "en-us" | "en" => EN_TOML,
            "es-es" | "es" => ES_TOML,
            _ => FR_TOML,
        };
        // Fallback merges every other language so any missing key still resolves.
        let mut fallback = load_toml(FR_TOML);
        fallback.extend(load_toml(EN_TOML));
        fallback.extend(load_toml(ES_TOML));
        Self {
            translations: load_toml(primary),
            fallback,
        }
    }
}

pub fn init(lang: &str) {
    INSTANCE.get_or_init(|| I18n::new(lang));
}

/// Get a translated string by key. Falls back to the other language, then to the key itself.
pub fn t(key: &str) -> &str {
    let i18n = INSTANCE.get_or_init(|| I18n::new("fr-fr"));
    if let Some(s) = i18n.translations.get(key) {
        return s.as_str();
    }
    if let Some(s) = i18n.fallback.get(key) {
        return s.as_str();
    }
    key
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_language_contains_the_portable_project_and_export_keys() {
        let required = [
            "menu.project.import.coquerythmo",
            "menu.export.mp4",
            "picker.project_save.title",
            "picker.delivery_export.title",
            "toast.save_requires_video",
            "toast.save_font_unavailable",
            "toast.legacy_project_loaded",
            "toast.export_requires_video",
            "export_hub.languages",
            "menu.tools.automation",
            "automation.add_entry",
            "automation.add_line_reroute",
            "automation.exec_input",
            "automation.enabled",
            "automation.add_role",
        ];
        for source in [FR_TOML, EN_TOML, ES_TOML] {
            let table = source
                .parse::<toml::Table>()
                .expect("valid translation TOML");
            for key in required {
                assert!(
                    table.get(key).is_some_and(toml::Value::is_str),
                    "missing translation key {key}"
                );
            }
        }
    }
}
