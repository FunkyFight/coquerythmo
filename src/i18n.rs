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
            "recording.actor_requests",
            "recording.actor_requests.open_microphone",
            "recording.actor_requests.transfer_project",
            "recording.actor_requests.transfer_display_settings",
            "recording.actor_requests.close_transfer_waiting",
            "recording.project_transfer.title",
            "recording.project_transfer.accept",
            "recording.project_transfer.save_replace",
            "recording.project_transfer.replace",
            "recording.project_transfer.refuse",
            "recording.project_transfer.no_project",
            "recording.project_transfer.save_and_transfer",
            "recording.project_transfer.load_waiting",
            "recording.project_transfer.dismissed",
            "loading_project.reading_manifest",
            "loading_project.extracting_assets",
            "loading_project.verifying_assets",
            "loading_project.preparing_project",
            "loading_project.ready",
            "automation.add_entry",
            "automation.add_line_reroute",
            "automation.exec_input",
            "automation.enabled",
            "automation.add_role",
            "file_tree.title",
            "file_tree.untitled",
            "file_tree.groups.videos",
            "file_tree.groups.bands",
            "file_tree.groups.audios",
            "file_tree.original_audio",
            "file_tree.rename",
            "file_tree.badges.proxy",
            "file_tree.badges.default",
            "file_tree.badges.has_proxy",
            "file_tree.badges.instrumental_of",
            "file_tree.menu.use",
            "file_tree.menu.make_default",
            "file_tree.menu.dissociate_proxy",
            "file_tree.menu.create_proxy",
            "file_tree.menu.recreate_proxy",
            "file_tree.menu.associate_proxy",
            "file_tree.menu.rename",
            "file_tree.menu.remove",
            "file_tree.menu.delete",
            "file_tree.menu.set_syllable_language",
            "file_tree.menu.set_instrumental",
            "file_tree.menu.no_eligible_video",
            "file_tree.menu.none",
            "toast.media_file_missing",
            "toast.media_save_project_first",
            "toast.media_default_saved",
            "toast.media_video_removed",
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

    #[test]
    fn every_language_contains_all_shortcut_names() {
        let french = FR_TOML
            .parse::<toml::Table>()
            .expect("valid French translations");
        let shortcut_keys: Vec<_> = french
            .keys()
            .filter(|key| key.starts_with("shortcut."))
            .cloned()
            .collect();
        assert!(!shortcut_keys.is_empty());
        for source in [EN_TOML, ES_TOML] {
            let table = source
                .parse::<toml::Table>()
                .expect("valid translation TOML");
            for key in &shortcut_keys {
                assert!(
                    table.get(key).is_some_and(toml::Value::is_str),
                    "missing translation key {key}"
                );
            }
        }
    }
}
