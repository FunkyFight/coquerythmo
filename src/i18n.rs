use std::collections::HashMap;
use std::sync::OnceLock;

static INSTANCE: OnceLock<I18n> = OnceLock::new();

struct I18n {
    translations: HashMap<&'static str, &'static str>,
}

impl I18n {
    fn new() -> Self {
        let mut translations = HashMap::new();

        // Topbar menus
        translations.insert("menu.project", "Projet");
        translations.insert("menu.project.add_video", "Ajouter une vidéo");

        // Zones placeholder
        translations.insert("zone.video_preview", "Aperçu vidéo");
        translations.insert("zone.rythmo", "Bande rythmo");
        translations.insert("zone.properties", "Propriétés");

        // Toolbar
        translations.insert("toolbar.play", "Lecture");
        translations.insert("toolbar.stop", "Pause");
        translations.insert("toolbar.volume", "Volume");
        translations.insert("toolbar.prev_frame", "Image précédente");
        translations.insert("toolbar.next_frame", "Image suivante");

        // Toolbar — markers
        translations.insert("toolbar.boucle", "Ajouter une boucle");
        translations.insert("toolbar.out", "Ajouter un out");
        translations.insert("toolbar.scene", "Changement de scène");
        translations.insert("toolbar.respirations", "Respirations");
        translations.insert("toolbar.reactions", "Réactions");
        translations.insert("toolbar.liaison_left", "Liaison à gauche");
        translations.insert("toolbar.liaison_right", "Liaison à droite");

        // Respirations tooltips
        translations.insert("resp.up", "Inspiration");
        translations.insert("resp.down", "Expiration");
        translations.insert("resp.h", "Respiration audible");
        translations.insert("resp.hh", "Respiration forte");
        translations.insert("resp.mh", "Micro respiration");
        translations.insert("resp.mhh", "Micro respiration forte");

        // Réactions tooltips
        translations.insert("react.x", "Bruit de bouche");
        translations.insert("react.mts", "Claquement de langue");
        translations.insert("react.tsc", "Désapprobation");
        translations.insert("react.ah", "Surprise");
        translations.insert("react.oh", "Étonnement");
        translations.insert("react.ih", "Dégoût");
        translations.insert("react.mhm", "Acquiescement");
        translations.insert("react.hm", "Hésitation");
        translations.insert("react.ptt", "Dédain");
        translations.insert("react.pff", "Exaspération");
        translations.insert("react.unh", "Effort");
        translations.insert("react.hun", "Interrogation");
        translations.insert("react.psst", "Interpellation");

        // File picker
        translations.insert("picker.video.title", "Choisir une vidéo");

        Self { translations }
    }
}

pub fn t(key: &str) -> &str {
    INSTANCE
        .get_or_init(I18n::new)
        .translations
        .get(key)
        .unwrap_or(&key)
}
