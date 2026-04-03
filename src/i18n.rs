use std::collections::HashMap;
use std::sync::OnceLock;

static INSTANCE: OnceLock<I18n> = OnceLock::new();

struct I18n {
    translations: HashMap<&'static str, &'static str>,
}

impl I18n {
    fn new(lang: &str) -> Self {
        let mut translations = HashMap::new();

        match lang {
            "en-us" | "en" => Self::load_english(&mut translations),
            _ => Self::load_french(&mut translations),
        }

        Self { translations }
    }

    fn load_french(t: &mut HashMap<&'static str, &'static str>) {
        // Topbar menus
        t.insert("menu.project", "Projet");
        t.insert("menu.project.add_video", "Ajouter une vidéo");
        t.insert("menu.project.import", "Importer une bande rythmo");
        t.insert("menu.project.export", "Exporter une bande rythmo");
        t.insert("picker.import.title", "Importer un projet");
        t.insert("picker.export.title", "Exporter le projet");

        // Export menu
        t.insert("menu.export", "Export");
        t.insert("menu.export.mp4", "Exporter le projet en MP4");
        t.insert("picker.export_mp4.title", "Enregistrer la vidéo MP4");

        // Connect menu
        t.insert("menu.connect", "Connexion");
        t.insert("menu.connect.create_room", "Créer un salon");
        t.insert("menu.connect.join_room", "Rejoindre un salon");
        t.insert("menu.connect.disconnect", "Se déconnecter");

        // Zones placeholder
        t.insert("zone.video_preview", "Aperçu vidéo");
        t.insert("zone.rythmo", "Bande rythmo");
        t.insert("zone.properties", "Propriétés");

        // Toolbar
        t.insert("toolbar.play", "Lecture");
        t.insert("toolbar.stop", "Pause");
        t.insert("toolbar.volume", "Volume");
        t.insert("toolbar.prev_frame", "Image précédente");
        t.insert("toolbar.next_frame", "Image suivante");

        // Toolbar — markers
        t.insert("toolbar.boucle", "Ajouter une boucle");
        t.insert("toolbar.out", "Ajouter un out");
        t.insert("toolbar.scene", "Changement de scène");
        t.insert("toolbar.respirations", "Respirations");
        t.insert("toolbar.reactions", "Réactions");
        t.insert("toolbar.liaison_left", "Liaison à gauche");
        t.insert("toolbar.liaison_right", "Liaison à droite");

        // Respirations tooltips
        t.insert("resp.up", "Inspiration");
        t.insert("resp.down", "Expiration");
        t.insert("resp.h", "Bouche ouverte, inspiration");
        t.insert("resp.hh", "Bouche ouverte, expiration");
        t.insert("resp.mh", "Bouche fermée, inspiration");
        t.insert("resp.mhh", "Bouche fermée, expiration");

        // Réactions tooltips
        t.insert("react.x", "Bruit de bouche");
        t.insert("react.mts", "Claquement de langue à l'ouverture");
        t.insert("react.tsc", "Claquement de bouche, bouche ouverte");
        t.insert("react.ah", "Surprise");
        t.insert("react.oh", "Étonnement");
        t.insert("react.ih", "Dégoût");
        t.insert("react.mhm", "Acquiescement");
        t.insert("react.hm", "Hésitation");
        t.insert("react.ptt", "Dédain");
        t.insert("react.pff", "Exaspération");
        t.insert("react.unh", "Effort");
        t.insert("react.hun", "Interrogation");
        t.insert("react.psst", "Interpellation");

        // File picker
        t.insert("picker.video.title", "Choisir une vidéo");

        // Settings
        t.insert("settings.title", "Paramètres");
        t.insert("settings.language", "Langue");
        t.insert("settings.rythmo_font", "Police de la bande rythmo");
        t.insert("settings.save", "Enregistrer");
        t.insert("settings.default_font", "Police par défaut");
        t.insert("settings.preview", "Aperçu");
        t.insert("settings.restart_required", "(redémarrage nécessaire)");
        t.insert("settings.tooltip", "Paramètres");
    }

    fn load_english(t: &mut HashMap<&'static str, &'static str>) {
        // Topbar menus
        t.insert("menu.project", "Project");
        t.insert("menu.project.add_video", "Add a video");
        t.insert("menu.project.import", "Import a rythmo band");
        t.insert("menu.project.export", "Export a rythmo band");
        t.insert("picker.import.title", "Import a project");
        t.insert("picker.export.title", "Export the project");

        // Export menu
        t.insert("menu.export", "Export");
        t.insert("menu.export.mp4", "Export project as MP4");
        t.insert("picker.export_mp4.title", "Save MP4 video");

        // Connect menu
        t.insert("menu.connect", "Connect");
        t.insert("menu.connect.create_room", "Create a room");
        t.insert("menu.connect.join_room", "Join a room");
        t.insert("menu.connect.disconnect", "Disconnect");

        // Zones placeholder
        t.insert("zone.video_preview", "Video preview");
        t.insert("zone.rythmo", "Rythmo band");
        t.insert("zone.properties", "Properties");

        // Toolbar
        t.insert("toolbar.play", "Play");
        t.insert("toolbar.stop", "Pause");
        t.insert("toolbar.volume", "Volume");
        t.insert("toolbar.prev_frame", "Previous frame");
        t.insert("toolbar.next_frame", "Next frame");

        // Toolbar — markers
        t.insert("toolbar.boucle", "Add a loop");
        t.insert("toolbar.out", "Add an out");
        t.insert("toolbar.scene", "Scene change");
        t.insert("toolbar.respirations", "Breathing");
        t.insert("toolbar.reactions", "Reactions");
        t.insert("toolbar.liaison_left", "Left liaison");
        t.insert("toolbar.liaison_right", "Right liaison");

        // Respirations tooltips
        t.insert("resp.up", "Inhale");
        t.insert("resp.down", "Exhale");
        t.insert("resp.h", "Mouth open, inhale");
        t.insert("resp.hh", "Mouth open, exhale");
        t.insert("resp.mh", "Mouth closed, inhale");
        t.insert("resp.mhh", "Mouth closed, exhale");

        // Reactions tooltips
        t.insert("react.x", "Mouth noise");
        t.insert("react.mts", "Tongue click on opening");
        t.insert("react.tsc", "Mouth click, mouth open");
        t.insert("react.ah", "Surprise");
        t.insert("react.oh", "Astonishment");
        t.insert("react.ih", "Disgust");
        t.insert("react.mhm", "Agreement");
        t.insert("react.hm", "Hesitation");
        t.insert("react.ptt", "Contempt");
        t.insert("react.pff", "Exasperation");
        t.insert("react.unh", "Effort");
        t.insert("react.hun", "Questioning");
        t.insert("react.psst", "Getting attention");

        // File picker
        t.insert("picker.video.title", "Choose a video");

        // Settings
        t.insert("settings.title", "Settings");
        t.insert("settings.language", "Language");
        t.insert("settings.rythmo_font", "Rythmo band font");
        t.insert("settings.save", "Save");
        t.insert("settings.default_font", "Default font");
        t.insert("settings.preview", "Preview");
        t.insert("settings.restart_required", "(restart required)");
        t.insert("settings.tooltip", "Settings");
    }
}

pub fn init(lang: &str) {
    INSTANCE.get_or_init(|| I18n::new(lang));
}

pub fn t(key: &str) -> &str {
    INSTANCE
        .get_or_init(|| I18n::new("fr-fr"))
        .translations
        .get(key)
        .unwrap_or(&key)
}
