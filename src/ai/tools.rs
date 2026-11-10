//! The catalogue of tools Needle 3 may call, sent to the engine as JSON
//! function schemas.
//!
//! Every tool here is executed in [`super::dispatch`]; the catalogue test
//! checks both directions. `TOOLS_NEEDLE3.md` is the human-readable copy of
//! this list and must be updated with it.
//!
//! Design rules, learnt from the engine (a 121 M-parameter model, weak in
//! French):
//! - The choice of the tool is made by the words of the query
//!   ([`super::retrieval`]), not by the model: every tool declares `triggers`,
//!   lower-case accent-free stems (French, English, Spanish) matched at a
//!   word start, verbs and objects alike ("ajoute" + "boucle"). A tool nobody
//!   can reach by its triggers never runs.
//! - Prefer several zero-argument tools (`add_loop`, `add_out_point`) to one
//!   tool with an enumerated argument: the words pick the right one, the
//!   model would not.
//! - Keep the arguments to what the sentence says (text, name, number). An
//!   optional argument the user did not mention is invented by the model and
//!   the whole call gets dropped as ungrounded.
//! - Names are English `snake_case` verbs; descriptions are short English
//!   sentences with the French words in parentheses, from which the model
//!   picks the arguments.

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ParamKind {
    Text,
    Number,
    Integer,
    Bool,
    Choice(&'static [&'static str]),
}

#[derive(Clone, Copy, Debug)]
pub struct ToolParam {
    pub name: &'static str,
    pub kind: ParamKind,
    pub description: &'static str,
    pub required: bool,
}

/// What the tool does, for the catalogue file and the help message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolKind {
    /// Changes the project (undoable through `Ctrl + Z`).
    Edit,
    /// Moves the playhead, the volume or the audio.
    Playback,
    /// Opens a panel, a window or a menu.
    Open,
    /// Answers a question without changing anything.
    Info,
    /// Saves, loads, imports, exports.
    Project,
}

#[derive(Clone, Copy, Debug)]
pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub params: &'static [ToolParam],
    pub triggers: &'static [&'static str],
    /// The keyboard shortcut or menu path of the same action, as written in
    /// the shortcut list, for the `get_shortcut` tool.
    pub shortcut: Option<&'static str>,
    pub kind: ToolKind,
    /// Sent to the engine when no trigger of any tool matches the query.
    pub fallback: bool,
}

impl ToolSpec {
    pub fn param(&self, name: &str) -> Option<&ToolParam> {
        self.params.iter().find(|param| param.name == name)
    }
}

const fn opt(name: &'static str, kind: ParamKind, description: &'static str) -> ToolParam {
    ToolParam {
        name,
        kind,
        description,
        required: false,
    }
}

const fn req(name: &'static str, kind: ParamKind, description: &'static str) -> ToolParam {
    ToolParam {
        name,
        kind,
        description,
        required: true,
    }
}

const fn tool(
    name: &'static str,
    kind: ToolKind,
    description: &'static str,
    params: &'static [ToolParam],
    triggers: &'static [&'static str],
    shortcut: Option<&'static str>,
) -> ToolSpec {
    ToolSpec {
        name,
        description,
        params,
        triggers,
        shortcut,
        kind,
        fallback: false,
    }
}

const fn fallback_tool(
    name: &'static str,
    kind: ToolKind,
    description: &'static str,
    params: &'static [ToolParam],
    triggers: &'static [&'static str],
) -> ToolSpec {
    ToolSpec {
        name,
        description,
        params,
        triggers,
        shortcut: None,
        kind,
        fallback: true,
    }
}

use ParamKind::{Choice, Integer, Number, Text};
use ToolKind::{Edit, Info, Open, Playback, Project};

const TIME: &str = "The position exactly as the user wrote it, unchanged and unconverted: 1:30, 1 minute 30, 00:01:30:12, 90s, frame 120";
const TRACK: &str = "Track number, 1 to 4";
const CHARACTER: &str = "Character (role) name";

/// Every tool of the Bande rythmo workspace.
pub const CATALOG: &[ToolSpec] = &[
    // ---- Playback -------------------------------------------------------
    tool("play", Playback, "Start the video playback (lecture, lire, play).", &[],
        &["play", "lecture", "lire", "lis ", "lance", "demarre", "joue", "reproduc", "reprend", "resume", "start", "go"],
        Some("Espace")),
    tool("pause", Playback, "Pause or stop the video playback (pause, stop, arrêter).", &[],
        &["pause", "stop", "arret", "stoppe", "deten", "para ", "parar", "fige", "stoppe la lecture", "arrete la lecture", "stoppe tout", "stop playback"],
        Some("Espace")),
    tool("play_reverse", Playback, "Play the video backwards (lecture inversée, à l'envers).", &[],
        &["envers", "inverse", "reverse", "backward", "arriere", "reves", "rebobin", "rewind", "rembobin"],
        Some("Maj + Espace")),
    tool("seek_to_time", Playback, "Move the playhead to a time position (aller à, va à, go to, ir a).",
        &[req("time", Text, TIME)],
        &["va a", "vas a", "aller a", "allez a", "go to", "goto", "jump to", "saute a", "ir a", "ve a", "salta a", "place toi", "mets toi", "rends toi", "positionne toi", "timecode", "minute", "tete de lecture", "playhead", "curseur de lecture", "second ", "seconds", "seconde"],
        Some("Clic sur la timeline")),
    tool("seek_relative", Playback, "Move the playhead forward or backward by a duration said in the query (avance de, recule de).",
        &[req("seconds", Number, "Seconds to move, negative goes backward")],
        &["avance de", "recule de", "forward", "backward", "en avant", "en arriere", "plus loin", "adelant", "retroced", "atras", "skip", "seconde", "segundo", "second ", "seconds"],
        None),
    tool("next_frame", Playback, "Go to the next frame (image suivante).", &[],
        &["image suivante", "next frame", "frame suivante", "fotograma siguiente", "siguiente fotograma", "une image", "avance d une image"],
        Some("Ctrl + →")),
    tool("previous_frame", Playback, "Go to the previous frame (image précédente).", &[],
        &["image precedente", "previous frame", "frame precedente", "fotograma anterior", "anterior fotograma", "recule d une image", "image d avant"],
        Some("Ctrl + ←")),
    tool("step_frames", Playback, "Move the playhead by a number of frames said in the query (avance de 10 images).",
        &[req("count", Integer, "Frames to move, negative goes backward")],
        &["images", "frames", "fotogramas", "trames", "avance de", "recule de", "images ", "frames "],
        Some("Ctrl + ← / Ctrl + →")),
    tool("go_to_start", Playback, "Move the playhead to the beginning of the video (début, start).", &[],
        &["debut", "start", "beginning", "inicio", "principio", "commencement", "zero", "retour au", "tout au debut", "va au debut", "aller au debut", "reviens au debut", "go to the start", "go to the beginning", "ve al inicio"],
        None),
    tool("go_to_end", Playback, "Move the playhead to the end of the video (fin, end).", &[],
        &["fin ", "fin de", "end", "final", "bout", "termin", "tout a la fin", "va a la fin", "aller a la fin", "va au bout", "go to the end", "ve al final"],
        None),
    tool("go_to_next_loop", Playback, "Jump to the next loop marker (boucle suivante, prochaine boucle).", &[],
        &["boucle suivante", "prochaine boucle", "next loop", "bucle siguiente", "siguiente bucle", "boucle d apres", "va a la boucle", "aller a la boucle", "go to the loop", "saute a la boucle"],
        Some("Ctrl + Maj + molette")),
    tool("go_to_previous_loop", Playback, "Jump to the previous loop marker (boucle précédente).", &[],
        &["boucle precedente", "boucle d avant", "previous loop", "bucle anterior", "derniere boucle", "last loop", "reviens a la boucle", "retour a la boucle"],
        Some("Ctrl + Maj + molette")),
    tool("set_volume", Playback, "Set the sound volume to the percentage said in the query (volume à 30, son à 50 %).",
        &[req("percent", Number, "0 to 100")],
        &["volume a", "volume de", "son a", "le son a", "volume", "sound to", "volumen a", "pour cent", "percent"],
        Some("Maj + ↑ / Maj + ↓")),
    tool("volume_up", Playback, "Raise the volume a little (monte le son, plus fort, louder).", &[],
        &["monte le son", "monte le volume", "plus fort", "louder", "volume up", "turn up", "sube el volumen", "mas alto", "augmente le son", "augmente le volume", "pas assez fort", "trop bas", "too quiet", "on n entend rien"],
        Some("Maj + ↑")),
    tool("volume_down", Playback, "Lower the volume a little (baisse le son, moins fort, quieter).", &[],
        &["baisse le son", "baisse le volume", "moins fort", "quieter", "volume down", "turn down", "baja el volumen", "mas bajo", "diminue le son", "diminue le volume", "trop fort", "son est trop fort", "too loud", "demasiado alto"],
        Some("Maj + ↓")),
    tool("mute", Playback, "Mute the sound (muet, coupe le son, silence).", &[],
        &["mute", "muet", "coupe le son", "couper le son", "silence", "sans son", "silencia", "enleve le son", "plus de son", "sourdine", "coupe moi le son", "mute the sound"],
        Some("Maj + Numpad -")),
    tool("unmute", Playback, "Unmute the sound (remets le son, unmute).", &[],
        &["unmute", "remets le son", "remet le son", "remettre le son", "reactive le son", "son revient", "rends le son", "activa el sonido"],
        Some("Maj + Numpad -")),
    tool("switch_audio", Playback, "Switch between the original audio and the instrumental audio (VO, instrumental).", &[],
        &["instrumental", "original", "vo ", "version originale", "piste audio", "audio track", "bascule l audio", "switch audio", "musique seule", "voix", "passe en vo", "en vo", "mets la version instrumentale", "passe a l instrumental", "ecoute l instrumental", "bascule sur l instrumental", "mets l instrumental"],
        Some("Ctrl + Tab")),
    tool("offset_audio", Playback, "Shift the active audio track by the milliseconds said in the query (décale l'audio de).",
        &[req("milliseconds", Number, "Offset in milliseconds, negative plays the audio earlier")],
        &["decale l audio", "decaler l audio", "audio offset", "offset audio", "delay audio", "retard audio", "synchro audio", "desfase", "calage audio", "milliseconde", "ms ", "decale le son", "decaler le son", "decale la piste audio", "shift the audio"],
        None),
    tool("open_secondary_display", Open, "Open the secondary window for the actors (fenêtre secondaire, deuxième écran).", &[],
        &["ouvre la fenetre secondaire", "fenetre secondaire", "deuxieme ecran", "second ecran", "secondary", "second screen", "second window", "ecran des", "pantalla secundaria", "comedien", "acteur"],
        Some("Menu Affichage")),
    tool("close_secondary_display", Open, "Close the secondary window (fermer la fenêtre secondaire).", &[],
        &["ferme la fenetre secondaire", "ferme le deuxieme ecran", "close the secondary", "cierra la pantalla secundaria", "ferme la fenetre"],
        Some("Échap dans la fenêtre secondaire")),
    // ---- Selection ------------------------------------------------------
    tool("select_line_at_playhead", Edit, "Select the dialogue line under the playhead (sélectionne la réplique ici).", &[],
        &["selectionne la ligne ici", "selectionne la replique ici", "ligne sous le curseur", "replique sous le curseur", "ligne au curseur", "line at the playhead", "line here", "ligne courante", "replique courante", "current line", "selectionne ici"],
        Some("Entrée")),
    tool("select_next_line", Edit, "Select the next dialogue line (réplique suivante).", &[],
        &["replique suivante", "ligne suivante", "dialogue suivant", "next line", "linea siguiente", "siguiente linea", "replique d apres", "passe a la suivante"],
        Some("Maj + →")),
    tool("select_previous_line", Edit, "Select the previous dialogue line (réplique précédente).", &[],
        &["replique precedente", "ligne precedente", "dialogue precedent", "previous line", "linea anterior", "anterior linea", "replique d avant", "reviens a la precedente"],
        Some("Maj + ←")),
    tool("select_all", Edit, "Select all the dialogue lines (tout sélectionner).", &[],
        &["tout selectionner", "selectionne tout", "selectionner tout", "toutes les lignes", "toutes les repliques", "tous les dialogues", "select all", "all lines", "all the lines", "everything", "selecciona todo", "todas las lineas"],
        Some("Ctrl + A")),
    tool("select_lines", Edit, "Select the dialogue lines of a character, or containing a text (sélectionne les répliques de).",
        &[opt("character", Text, CHARACTER), opt("text", Text, "Text contained in the line, only if the query quotes one")],
        &["selectionne", "selection", "select", "selecciona", "les repliques de", "les lignes de", "the lines of", "las lineas de", "contenant", "containing", "qui contiennent"],
        Some("Maj + clic-glissé")),
    tool("clear_selection", Edit, "Clear the line selection (désélectionner, deselect).", &[],
        &["deselect", "annule la selection", "enleve la selection", "selectionne rien", "plus rien", "deseleccion", "retire la selection"],
        Some("Échap")),
    // ---- Lines ----------------------------------------------------------
    tool("create_line", Edit, "Create a new dialogue line at the playhead with the text, character and track said in the query (crée une réplique de Marie sur la piste 2 qui dit ...).",
        &[opt("text", Text, "Dialogue text, if the query says it"), opt("character", Text, CHARACTER), opt("track", Integer, TRACK)],
        &["cree", "creer", "create", "nouvelle", "nouveau", "new", "ajoute", "add", "ecris", "write", "crea", "nueva", "phrase", "sentence", "frase", "cree une replique", "cree une ligne", "cree un dialogue", "creer une replique", "creer une ligne", "cree la replique", "cree la ligne", "nouvelle replique", "nouvelle ligne", "nouveau dialogue", "ajoute une replique", "ajoute une ligne", "ajoute un dialogue", "une ligne pour", "une replique pour", "une replique de", "une ligne de", "new line", "a new line", "create a line", "add a line", "crea una linea", "nueva linea", "anade una linea"],
        Some("Ctrl + Numpad 1 à 4, ou clic maintenu sur la bande")),
    tool("set_line_text", Edit, "Replace the text of the selected dialogue line (change le texte en, remplace par).",
        &[req("text", Text, "New text of the line")],
        &["change le texte", "remplace le texte", "remplace par", "replace with", "nouveau texte", "texte de la ligne", "change the text", "cambia el texto", "corrige", "fix the text", "ecris a la place", "dis plutot"],
        Some("T")),
    tool("edit_line_text", Edit, "Open the text editor of the selected line (modifier le texte, éditer).", &[],
        &["modifie le texte", "modifier le texte", "edite le texte", "edit the text", "edita el texto", "retouche le texte", "ouvre le texte", "corriger le texte", "editer le texte", "edite la ligne", "modifie la ligne"],
        Some("T")),
    tool("edit_line_sections", Edit, "Edit the selected line section by section, keeping the sync points and karaoke timings (édition par section).", &[],
        &["section", "par section", "morceau par morceau", "partie par partie", "sans toucher", "garde le calage", "sans perdre"],
        Some("Ctrl + T")),
    tool("set_line_character", Edit, "Give the selected lines the character said in the query (attribue à Marie, c'est Jean qui parle).",
        &[req("character", Text, CHARACTER)],
        &["attribue", "attribuer", "assign", "asigna", "c est ", "qui parle", "speaker", "parle", "met le personnage", "mets le personnage", "donne le personnage", "le personnage a", "pour le personnage", "dite par", "dit par", "said by", "spoken by", "dicha por", "qui dit cette", "dit cette phrase", "dit cette replique"],
        Some("P")),
    tool("edit_line_character", Edit, "Open the character field of the selected line (modifier le personnage).", &[],
        &["modifie le personnage", "modifier le personnage", "edit the character", "edita el personaje", "change le personnage", "champ personnage"],
        Some("P")),
    tool("delete_selected_lines", Edit, "Delete the selected dialogue lines (supprime la sélection, delete).", &[],
        &["supprime", "supprimer", "efface", "delete", "remove", "retire", "borra", "elimina", "enleve", "vire", "degage", "la selection", "selected"],
        Some("Suppr")),
    tool("delete_lines", Edit, "Delete the lines of a character or containing a text (supprime toutes les répliques de Marie).",
        &[opt("character", Text, CHARACTER), opt("text", Text, "Text contained in the lines, only if the query quotes one")],
        &["supprime toutes", "supprime les", "supprimer les", "efface les", "efface toutes", "delete all", "delete the lines", "remove the lines", "borra las", "elimina las", "les repliques de", "les lignes de", "the lines of", "las lineas de"],
        Some("Panneau des lignes")),
    tool("copy_lines", Edit, "Copy the selected lines (copier, copy).", &[],
        &["copie", "copier", "copy", "copia", "duplique", "duplicate"],
        Some("Ctrl + C")),
    tool("cut_lines", Edit, "Cut the selected lines (couper, cut).", &[],
        &["coupe la", "coupe les", "couper", "cut", "corta", "cortar"],
        Some("Ctrl + X")),
    tool("paste_lines", Edit, "Paste the copied lines at the playhead (coller, paste).", &[],
        &["colle", "coller", "paste", "pega", "pegar"],
        Some("Ctrl + V")),
    tool("paste_lines_with_track_character", Edit, "Paste the copied lines with the character of the track (coller avec le personnage de la piste).", &[],
        &["colle avec le personnage", "coller avec le personnage", "paste with the character", "personnage de la piste", "character of the track"],
        Some("Ctrl + Maj + V")),
    tool("move_lines_to_track", Edit, "Move the selected lines to the track number said in the query (déplace sur la piste 3).",
        &[req("track", Integer, TRACK)],
        &["piste", "track", "pista", "deplace", "move", "bouge", "monte", "descend", "up", "down", "change de piste", "vers la piste", "to track", "a la pista"],
        Some("↑ / ↓")),
    tool("move_lines_to_time", Edit, "Move the selected lines to the time said in the query (positionne-la à 1:30, place-les au timecode).",
        &[req("time", Text, TIME)],
        &["positionne la a", "positionne les a", "place la a", "place les a", "mets la a", "mets les a", "move it to", "move them to", "put it at", "a la position", "au timecode", "a la meme position", "same time", "colocala en", "ponla en", "positionne a", "place a", "place la ligne a", "mets la ligne a", "deplace la ligne a", "positionne la ligne a", "place la replique a", "deplace la replique a", "deplace la replique au", "place la ligne au"],
        Some("Glisser la ligne")),
    tool("move_lines_to_character_line", Edit, "Move the selected lines to the start of a line of another character (positionne-la au niveau de la première réplique de Chanson).",
        &[req("character", Text, "Character whose line gives the position"), opt("line_index", Integer, "Which line of that character, 1 = first, -1 = last, only if said")],
        &["au niveau de", "a la hauteur de", "en face de", "aligne sur", "aligne avec", "align with", "au meme endroit que", "same place as", "a la altura de", "premiere ligne de", "premiere replique de", "derniere ligne de", "derniere replique de", "first line of", "last line of", "en meme temps que"],
        Some("Glisser la ligne")),
    tool("shift_lines", Edit, "Shift the selected lines by the duration said in the query, without changing their duration (décale de 2 secondes, nudge).",
        &[opt("seconds", Number, "Seconds to shift, negative to the left"), opt("frames", Integer, "Frames to shift, negative to the left")],
        &["decale", "decaler", "shift", "nudge", "pousse", "desplaza", "plus tot", "plus tard", "earlier", "later", "vers la gauche", "vers la droite", "to the left", "to the right", "a gauche", "a droite"],
        Some("Ctrl + Maj + ← / →")),
    tool("set_line_start", Edit, "Set the start of the selected line at the playhead (début ici, point d'entrée).", &[],
        &["debut ici", "debut de la ligne", "debut de la replique", "commence ici", "start here", "set the start", "entree ici", "inicio aqui", "empieza aqui", "fixe le debut", "fixer le debut", "demarre ici"],
        Some("I")),
    tool("set_line_end", Edit, "Set the end of the selected line at the playhead (fin ici, point de sortie).", &[],
        &["fin ici", "fin de la ligne", "fin de la replique", "finit ici", "termine ici", "end here", "set the end", "sortie ici", "final aqui", "termina aqui", "fixe la fin", "fixer la fin", "s arrete ici"],
        Some("O")),
    tool("set_line_duration", Edit, "Set the duration of the selected line to the seconds said in the query (dure 3 secondes).",
        &[req("seconds", Number, "Duration in seconds")],
        &["duree", "dure", "duration", "length", "longueur", "duracion", "allonge", "raccourcis", "shorten", "lengthen", "etire", "stretch", "secondes de long", "la ligne dure", "la replique dure", "the line lasts", "dure "],
        Some("Glisser le bord de la ligne")),
    tool("split_line", Edit, "Split the dialogue line in two at the caret or the playhead (scinder, couper en deux, split).", &[],
        &["scinde", "scinder", "split", "en deux", "coupe en deux", "separe en", "divise", "divide", "parte en", "separe la", "separe la replique", "separe la ligne"],
        Some("Ctrl + K")),
    tool("toggle_karaoke", Edit, "Turn the karaoke mode on or off for the selected lines (karaoké).", &[],
        &["karaoke"],
        Some("Numpad 9")),
    tool("toggle_bold", Edit, "Toggle bold on the selected line or text (gras, bold).", &[],
        &["gras", "bold", "negrita", "en gras", "in bold"],
        Some("Ctrl + B")),
    tool("toggle_italic", Edit, "Toggle italic on the selected line or text (italique, italic).", &[],
        &["italique", "italic", "cursiva", "en italique", "in italics"],
        Some("Ctrl + I pendant l'édition")),
    tool("toggle_underline", Edit, "Toggle underline on the selected line or text (souligné, underline).", &[],
        &["souligne", "souligner", "underline", "subray", "soulignement"],
        Some("Ctrl + U")),
    tool("toggle_strikethrough", Edit, "Toggle strikethrough on the selected line or text (barré, strikethrough).", &[],
        &["barre", "barrer", "strike", "strikethrough", "tachad", "raye", "rayer"],
        Some("Barre de mise en forme")),
    tool("set_text_emotion", Edit, "Animate the selected line with the emotion said in the query (colère, joie, peur, tristesse, doute, question, exclamation, tremblement, vague, rebond...).",
        &[req("emotion", Text, "Emotion or effect name as said, with its intensity word if any")],
        &["emotion", "colere", "anger", "angry", "enerve", "ira", "joie", "joy", "happy", "content", "alegr", "peur", "fear", "panique", "miedo",
          "triste", "sad", "amour", "love", "tendre", "degout", "disgust", "asco", "doute", "doubt", "duda", "question", "pregunta", "exclamation", "crie", "shout",
          "tremble", "shake", "vague", "wave", "rebond", "bounce", "balance", "swing", "pendule", "pendulum", "glisse", "slide", "oscill", "gigote", "wiggle", "anime", "animate", "effet"],
        Some("Alt + E")),
    tool("clear_text_emotion", Edit, "Remove the emotion of the selected line (enlève l'émotion, sans effet).", &[],
        &["enleve l emotion", "retire l emotion", "sans emotion", "sans effet", "remove the emotion", "no emotion", "quita la emocion", "calme", "normal", "plus d emotion", "retire l effet", "enleve l effet", "plus d effet", "remove the effect"],
        Some("Alt + E")),
    tool("open_emotion_menu", Open, "Open the text emotions menu (menu des émotions).", &[],
        &["menu des emotions", "menu emotion", "emotions menu", "ouvre les emotions", "liste des emotions", "menu de emociones"],
        Some("Alt + E")),
    tool("link_lines", Edit, "Link the selected dialogues so they share their text and timings (dialogues liés, lier).", &[],
        &["lie les", "lier les", "lie ces", "lier ces", "link the", "link these", "lies", "liee", "relie", "vincula", "enlaza", "dialogues lies", "linked dialogues", "synchronise les repliques", "meme texte"],
        Some("Clic droit > Dialogues liés")),
    tool("link_lines_with_character", Edit, "Link the selected dialogues, sharing the character too (lier avec le personnage).", &[],
        &["lie avec le personnage", "lier avec le personnage", "link with the character", "personnage inclus", "including the character", "meme personnage"],
        Some("Clic droit > Dialogues liés > Inclure le personnage")),
    tool("unlink_lines", Edit, "Unlink the selected linked dialogues (délier, unlink).", &[],
        &["delie", "delier", "unlink", "detache", "separe les dialogues", "desvincula"],
        Some("Clic droit > Dialogues liés > Délier")),
    tool("set_line_note", Edit, "Write the note said in the query on the selected line (note : ..., commentaire).",
        &[req("note", Text, "Note text")],
        &["note ", "commentaire", "comment", "remarque", "annotation", "nota", "indication", "ecris en note", "mets une note"],
        Some("Numpad 6")),
    tool("add_note", Edit, "Add a note to the selected line and start writing it (ajouter une note).", &[],
        &["ajoute une note", "ajouter une note", "nouvelle note", "add a note", "new note", "anade una nota", "ouvre la note"],
        Some("Numpad 6")),
    tool("set_line_on_screen", Edit, "Mark the selected line as spoken on screen (à l'écran, dans le champ, visible).", &[],
        &["a l ecran", "dans le champ", "en champ", "on screen", "visible", "en plano", "le voit", "on le voit"],
        Some("Palette de détection (Alt + D)")),
    tool("set_line_off_screen", Edit, "Mark the selected line as spoken off screen (hors champ, off, voix off).", &[],
        &["hors champ", "off screen", "offscreen", "voix off", "en off", "fuera de plano", "hors ecran", "pas a l ecran", "on ne le voit pas"],
        Some("Palette de détection (Alt + D)")),
    tool("set_line_back", Edit, "Mark the selected line as spoken from the back (de dos, back to camera).", &[],
        &["de dos", "dos a la camera", "back to camera", "from the back", "de espaldas", "il tourne le dos"],
        Some("Palette de détection (Alt + D)")),
    tool("generate_detection_signs", Edit, "Convert the selected line into lip detection signs (détection, signes, labiales).", &[],
        &["detection", "signes", "signs", "labial", "levres", "lips", "bouche", "mouth", "convertis", "genere les signes", "generate signs"],
        Some("Clic droit > Convertir en signes de détection")),
    tool("undo", Edit, "Undo the last edit (annuler, undo, retour).", &[],
        &["annule", "annuler", "undo", "defais", "retour en arriere", "deshacer", "deshace", "oups", "erreur", "mistake", "reviens en arriere"],
        Some("Ctrl + Z")),
    tool("redo", Edit, "Redo the last undone edit (rétablir, refaire, redo).", &[],
        &["retablis", "retablir", "refais", "refaire", "redo", "rehacer", "rehace"],
        Some("Ctrl + Y / Ctrl + Maj + Z")),
    // ---- Markers --------------------------------------------------------
    tool("add_loop", Edit, "Add a loop marker (boucle) at the playhead.", &[],
        &["boucle", "loop", "bucle", "ajoute", "ajouter", "add", "pose", "place", "cree", "insere", "insert", "mets", "anade", "agrega", "nouvelle", "nouveau", "new"],
        Some("Numpad 1")),
    tool("add_out_point", Edit, "Add an out point marker (point Out) at the playhead.", &[],
        &["point out", "out", "sortie", "ajoute", "ajouter", "add", "pose", "place", "cree", "insere", "insert", "mets", "anade", "agrega", "nouveau", "new"],
        Some("Numpad 2")),
    tool("add_scene_change", Edit, "Add a scene change marker (changement de plan) at the playhead.", &[],
        &["changement de plan", "changement de scene", "scene", "plan", "cut", "coupe de plan", "nouveau plan", "new scene", "cambio de plano", "ajoute", "ajouter", "add", "pose", "place", "mets", "insere"],
        Some("Numpad 3")),
    tool("add_liaison_left", Edit, "Add a liaison (ambiance link) to the left (liaison gauche).", &[],
        &["liaison gauche", "liaison a gauche", "liaison vers la gauche", "left liaison", "liaison", "liason", "ambiance", "enlace", "gauche", "left"],
        Some("Numpad 7")),
    tool("add_liaison_right", Edit, "Add a liaison (ambiance link) to the right (liaison droite).", &[],
        &["liaison droite", "liaison a droite", "liaison vers la droite", "right liaison", "liaison", "liason", "ambiance", "enlace", "droite", "right"],
        Some("Numpad 8")),
    tool("add_breath", Edit, "Add a breath mark line at the playhead (respiration, souffle, breath).",
        &[opt("kind", Choice(&["h", "hh", "mh", "mhh", "up", "down"]), "Breath kind only if the query says it (H, HH, mH, mHH, up, down)")],
        &["respir", "souffle", "breath", "inspir", "expir", "respiracion", "aliento", "soupir", "sigh"],
        Some("Numpad 4")),
    tool("add_reaction", Edit, "Add a reaction mark line at the playhead (réaction: rire, cri, ah, oh, pff, tsc, mhm...).",
        &[opt("kind", Choice(&["x", "mts", "tsc", "ah", "oh", "ih", "mhm", "hm", "ptt", "pff", "unh", "hun", "psst"]), "Reaction kind only if the query says it")],
        &["reaction", "rire", "laugh", "risa", "cri", "scream", "grito", "pff", "tsc", "mhm", "psst", "onomatop", "bruit de bouche", "grognement", "grunt"],
        Some("Numpad 5")),
    tool("add_sync_point", Edit, "Add a synchronization point at the playhead on the selected line (point de synchro).", &[],
        &["point de sync", "point de synchro", "synchro", "sync point", "sync ", "calage", "ancre", "anchor", "sincron"],
        Some("Ctrl + Maj + Espace")),
    tool("list_markers", Info, "Tell which markers exist: loops, out points, scene changes (liste des boucles, combien de marqueurs).", &[],
        &["liste des boucles", "liste les boucles", "liste des marqueurs", "liste les marqueurs", "combien de boucles", "combien de marqueurs", "how many loops", "how many markers", "list the loops", "list the markers", "quelles boucles", "which loops", "cuantos bucles", "ou sont les boucles", "where are the loops"],
        None),
    // ---- Characters -----------------------------------------------------
    tool("rename_character", Edit, "Rename a character everywhere: old name then new name as said in the query (renomme Jean en Pierre, Jean s'appelle Pierre).",
        &[req("old_name", Text, "Current name of the character"), req("new_name", Text, "New name of the character")],
        &["renomme", "renommer", "rename", "renombra", "s appelle", "appelle le", "appelle la", "devient", "becomes", "nom du personnage", "change le nom", "change the name", "nouveau nom", "renomme le personnage", "nom du role", "cambia el nombre", "nombre del personaje"],
        Some("Menu Outils > Renommer un personnage")),
    tool("set_character_color", Edit, "Change the colour of a character everywhere (couleur du personnage, mets Jean en rouge).",
        &[req("character", Text, CHARACTER), req("color", Text, "Colour name as said (rouge, blue, verde...) or hex code")],
        &["couleur", "color", "colour", "rouge", "bleu", "vert", "jaune", "orange", "violet", "rose", "red", "blue", "green", "yellow", "purple", "pink", "rojo", "azul", "verde", "en rouge", "en bleu", "en vert", "make ", "noir", "blanc", "gris", "marron", "turquoise", "black", "white", "grey", "gray", "brown", "en noir", "en blanc"],
        Some("Panneau des rôles (Ctrl + P)")),
    tool("list_characters", Info, "Tell which characters (roles) exist in the project (liste des personnages, qui parle, combien de personnages).", &[],
        &["liste des personnages", "liste les personnages", "quels personnages", "quels sont les personnages", "qui parle", "who speaks", "which characters", "list the characters", "combien de personnages", "how many characters", "cuantos personajes", "les personnages", "les roles", "the characters"],
        Some("Ctrl + P")),
    tool("open_roles_panel", Open, "Open the roles panel listing the characters and their colours (panneau des rôles, personnages).", &[],
        &["panneau des roles", "panneau des personnages", "roles panel", "panel de roles", "ouvre les roles", "ouvre les personnages", "tableau des roles", "les roles"],
        Some("Ctrl + P")),
    tool("open_lines_panel", Open, "Open the lines panel listing every dialogue (panneau des lignes, tableau des lignes, script).", &[],
        &["panneau des lignes", "tableau des lignes", "lines panel", "lines table", "liste des lignes", "liste des repliques", "lines list", "script", "ouvre les lignes", "ouvre les repliques", "panel de lineas"],
        Some("Ctrl + I")),
    tool("open_rename_character", Open, "Open the window to rename a character (fenêtre renommer un personnage).", &[],
        &["fenetre renommer", "fenetre de renommage", "rename window", "ouvre renommer", "renommer un personnage"],
        Some("Menu Outils > Renommer un personnage")),
    tool("open_voice_actors", Open, "Open the voice actors window (comédiens, voice actors, casting).", &[],
        &["comediens", "les comediens", "voice actors", "acteurs", "actrices", "casting", "doubleurs", "actores de voz", "ouvre les comediens", "fenetre des comediens"],
        Some("Menu Outils > Comédiens")),
    tool("create_voice_actor", Edit, "Create a voice actor with the name said in the query (crée un comédien Paul).",
        &[req("name", Text, "Actor name")],
        &["cree un comedien", "creer un comedien", "nouveau comedien", "ajoute un comedien", "create a voice actor", "new voice actor", "add a voice actor", "crea un actor", "nuevo actor", "cree une comedienne", "nouvelle comedienne"],
        Some("Menu Outils > Comédiens")),
    tool("assign_voice_actor_to_line", Edit, "Assign the voice actor said in the query to the selected line (attribue le comédien Paul à la ligne).",
        &[req("actor", Text, "Actor name")],
        &["attribue le comedien", "attribuer le comedien", "assign the actor", "assign the voice actor", "comedien a la ligne", "actor to the line", "joue par", "played by", "double par", "dubbed by", "asigna el actor", "doublee par", "est doublee", "voiced by"],
        Some("Clic droit > Attribuer à un comédien")),
    tool("assign_voice_actor_to_character", Edit, "Assign the voice actor said in the query to the whole character of the selected line (le comédien Paul pour tout le personnage).",
        &[req("actor", Text, "Actor name")],
        &["comedien au personnage", "comedien pour le personnage", "actor to the character", "tout le personnage", "whole character", "toutes ses repliques", "all his lines", "all her lines", "actor al personaje", "au personnage", "to the character", "al personaje"],
        Some("Clic droit > Attribuer à un comédien")),
    // ---- Drawing tools --------------------------------------------------
    tool("use_select_tool", Edit, "Switch to the selection tool (outil sélection, flèche).", &[],
        &["outil selection", "outil de selection", "mode selection", "selection tool", "select tool", "fleche", "arrow", "herramienta de seleccion", "arrete de dessiner", "stop drawing", "quitte le dessin"],
        Some("C")),
    tool("use_draw_tool", Edit, "Switch to the drawing tool (outil dessin, pinceau, crayon).", &[],
        &["outil dessin", "outil de dessin", "mode dessin", "draw tool", "drawing tool", "pinceau", "brush", "crayon", "pencil", "dessine", "draw", "dibuj", "herramienta de dibujo"],
        Some("Ctrl + D")),
    tool("toggle_eraser", Edit, "Toggle the eraser of the drawing tool (gomme, eraser).", &[],
        &["gomme", "eraser", "efface le dessin", "goma", "borrador", "efface les traits", "erase"],
        Some("Barre des outils de dessin")),
    tool("change_brush_size", Edit, "Cycle the brush size of the drawing tool (taille du pinceau, épaisseur du trait).", &[],
        &["taille du pinceau", "taille du trait", "brush size", "epaisseur", "thickness", "grosor", "plus gros", "plus fin", "bigger brush", "smaller brush"],
        Some("Barre des outils de dessin")),
    tool("open_brush_color", Open, "Open the brush colour picker (couleur du pinceau, du trait).", &[],
        &["couleur du pinceau", "couleur du trait", "couleur du dessin", "brush colour", "brush color", "color del pincel", "change la couleur du pinceau"],
        Some("Barre des outils de dessin")),
    // ---- Export and windows ---------------------------------------------
    tool("open_export", Open, "Open the export window: video, subtitles, audio, references (exporter, export).", &[],
        &["export", "exporte", "exporter", "rendu", "render", "mp4", "video finale", "livrable", "sortie video", "exporta"],
        Some("Ctrl + M")),
    tool("export_subtitles", Project, "Export the subtitles or the script (srt, ass, detx, json, pdf): opens the export window on the subtitles page.", &[],
        &["exporte les sous titres", "exporter les sous titres", "export the subtitles", "exporte en srt", "export srt", "srt", "ass ", "detx", "sous titres", "subtitles", "subtitulos", "exporte le script", "export the script", "exporte le texte", "pdf"],
        Some("Ctrl + M")),
    tool("cancel_export", Project, "Cancel the running export or proxy creation (annule l'export).", &[],
        &["annule l export", "annuler l export", "arrete l export", "stop the export", "cancel the export", "annule le proxy", "cancela la exportacion", "stoppe l export"],
        Some("Échap")),
    tool("open_proxy", Open, "Open the proxy creation window, a lighter copy of the video (proxy, vidéo légère).", &[],
        &["proxy", "video legere", "allege la video", "lighter video", "compresse la video", "basse resolution", "low res", "cree un proxy", "create a proxy"],
        Some("Ctrl + Maj + P")),
    tool("open_project_settings", Open, "Open the project settings: font, scroll speed, reading bar, timecode, waveform (paramètres du projet).", &[],
        &["parametres du projet", "parametre du projet", "project settings", "reglages du projet", "options du projet", "configuration du projet", "ajustes del proyecto", "ouvre les parametres", "ouvre les reglages", "police", "font"],
        Some("Ctrl + O")),
    tool("set_scroll_speed", Edit, "Set the scrolling speed of the band to the value said in the query (vitesse de défilement 1.5).",
        &[req("value", Number, "Speed factor, 1 is normal, between 0.25 and 4")],
        &["vitesse de defilement", "vitesse du defilement", "scroll speed", "scrolling speed", "velocidad de desplazamiento", "vitesse de la bande", "defilement a", "vitesse a"],
        Some("Ctrl + O")),
    tool("set_reading_bar_offset", Edit, "Move the reading bar to the percentage said in the query (barre de lecture à 10 %).",
        &[req("value", Number, "Percent, between -50 and 50")],
        &["barre de lecture", "reading bar", "barra de lectura", "decale la barre", "position de la barre"],
        Some("Ctrl + O")),
    tool("toggle_highlight_read_word", Edit, "Turn on or off the highlighting of the word being read (surligner le mot lu).", &[],
        &["surligne le mot", "surligner le mot", "mot lu", "highlight the word", "word highlight", "resaltar la palabra", "surlignage"],
        Some("Ctrl + O")),
    tool("toggle_character_color_text", Edit, "Turn on or off the character colour on the scrolling text (texte à la couleur du personnage).", &[],
        &["couleur du personnage sur le texte", "texte a la couleur", "texte en couleur", "text in the character colour", "character colour on text", "texto del color del personaje", "couleur de l etiquette"],
        Some("Ctrl + O")),
    tool("toggle_emotion_lanes", Edit, "Show or hide the emotion lanes (pistes d'émotions).", &[],
        &["pistes d emotion", "pistes d emotions", "lignes d emotion", "emotion lanes", "affiche les emotions", "masque les emotions", "cache les emotions", "carriles de emociones"],
        Some("Ctrl + O")),
    tool("toggle_timecode_display", Edit, "Show or hide the timecode in the toolbar (afficher le timecode).", &[],
        &["affiche le timecode", "afficher le timecode", "masque le timecode", "cache le timecode", "show the timecode", "hide the timecode", "muestra el timecode", "timecode dans la barre"],
        Some("Ctrl + O")),
    tool("set_start_timecode", Edit, "Set the timecode of the first frame to the value said in the query (timecode de départ 01:00:00:00).",
        &[req("value", Text, "Timecode of the first frame, as said")],
        &["timecode de depart", "timecode de debut", "timecode initial", "start timecode", "premiere image", "first frame", "timecode inicial", "commence au timecode"],
        Some("Ctrl + O")),
    tool("set_waveform_height", Edit, "Set the height of the waveform strip to the pixels said in the query (forme d'onde 80 px).",
        &[req("value", Number, "Height in pixels")],
        &["forme d onde", "hauteur de l onde", "waveform", "onde audio", "forma de onda", "hauteur de la forme"],
        Some("Ctrl + O")),
    tool("open_band_style", Open, "Open the band style window: colours, reading bar, karaoke dot, presets (style de la bande, apparence).", &[],
        &["style de la bande", "style de bande", "band style", "apparence de la bande", "look de la bande", "theme de la bande", "couleurs de la bande", "preset de style", "presets", "estilo de la banda", "ouvre le style", "aspect de la bande", "point karaoke", "karaoke dot"],
        Some("Ctrl + Maj + O")),
    tool("open_settings", Open, "Open the application settings: language, temporary folder, hints (préférences, paramètres de l'application).", &[],
        &["preferences", "parametres de l application", "parametres de l appli", "parametres de l app", "app settings", "application settings", "langue de l application", "change la langue", "change language", "idioma", "dossier temporaire", "temp folder", "ajustes de la aplicacion"],
        Some("Menu Paramètres")),
    tool("show_shortcuts", Open, "Show the keyboard shortcuts panel at the bottom left (affiche les raccourcis, aide-mémoire).", &[],
        &["affiche les raccourcis", "afficher les raccourcis", "montre les raccourcis", "show the shortcuts", "shortcuts panel", "panneau des raccourcis", "liste des raccourcis", "muestra los atajos", "aide memoire", "cheat sheet", "voir les raccourcis"],
        Some("Menu Paramètres > Afficher les raccourcis")),
    tool("hide_shortcuts", Open, "Hide the keyboard shortcuts panel (masque les raccourcis).", &[],
        &["masque les raccourcis", "cache les raccourcis", "enleve les raccourcis", "hide the shortcuts", "oculta los atajos", "ferme les raccourcis"],
        Some("Menu Paramètres > Afficher les raccourcis")),
    tool("open_automation", Open, "Open the automation graph that arranges lines by role onto tracks (automatisation, graphe).", &[],
        &["automati", "automation", "graphe", "graph", "noeud", "node", "blueprint", "regle de piste", "automatizacion"],
        Some("Menu Outils > Automatisation")),
    tool("toggle_project_files", Open, "Open or close the project files panel: videos, audios, bands (fichiers du projet, explorateur).", &[],
        &["fichiers du projet", "fichier du projet", "project files", "explorateur", "explorer", "arborescence", "tree", "medias du projet", "archivos del proyecto", "videos du projet", "ouvre les fichiers", "ferme les fichiers"],
        Some("Ctrl + L")),
    tool("open_recent_projects", Open, "Open the recent projects list (projets récents, derniers projets).", &[],
        &["projets recents", "projet recent", "recent projects", "derniers projets", "last projects", "proyectos recientes", "historique des projets", "ouvre les recents"],
        Some("Ctrl + R")),
    tool("close_panel", Open, "Close the open panel, menu or window (ferme le panneau, close).", &[],
        &["ferme le panneau", "fermer le panneau", "ferme la fenetre", "ferme le menu", "close the panel", "close the window", "cierra el panel", "range le panneau", "cache le panneau", "enleve le panneau", "ferme tout"],
        Some("Échap")),
    // ---- Project --------------------------------------------------------
    tool("save_project", Project, "Save the project (sauvegarder, enregistrer, save).", &[],
        &["sauvegarde", "sauvegarder", "sauve", "enregistre", "enregistrer", "save", "guarda", "guardar", "backup"],
        Some("Ctrl + S")),
    tool("save_project_as", Project, "Save the project to a new file (enregistrer sous, save as).", &[],
        &["enregistre sous", "enregistrer sous", "sauvegarde sous", "save as", "guardar como", "nouveau fichier", "copie du projet", "autre nom", "sous un autre nom"],
        Some("Menu Projet > Enregistrer sous")),
    tool("new_project", Project, "Create a new project (nouveau projet, new project).", &[],
        &["nouveau projet", "new project", "nuevo proyecto", "cree un projet", "creer un projet", "recommence a zero", "from scratch"],
        Some("Ctrl + N")),
    tool("open_project", Project, "Open a project file from the disk (ouvrir un projet, charger, load).", &[],
        &["ouvre un projet", "ouvrir un projet", "ouvre le projet", "open a project", "open project", "open the project", "charge un projet", "charger un projet", "load a project", "abre un proyecto", "abrir proyecto", "fichier coquerythmo"],
        Some("Ctrl + Maj + I")),
    tool("close_project", Project, "Close the project (fermer le projet, close project).", &[],
        &["ferme le projet", "fermer le projet", "close the project", "close project", "cierra el proyecto", "quitte le projet"],
        Some("Ctrl + Suppr")),
    tool("import_subtitles", Project, "Import subtitles or a script file: srt, ass, detx, json, coquerythmo (importer des sous-titres).", &[],
        &["importe", "importer", "import", "importa", "sous titrage", "fichier srt", "fichier ass", "fichier detx", "charge un fichier", "load a file", "charge des sous titres", "import subtitles", "import the subtitles", "importe des sous titres", "importe les sous titres", "importer des sous titres", "importa subtitulos"],
        Some("Ctrl + Maj + I")),
    tool("restore_backup", Project, "Restore the last automatic backup (restaurer la sauvegarde, récupérer).", &[],
        &["restaure", "restaurer", "restore", "recupere", "recover", "sauvegarde automatique", "derniere sauvegarde", "restaura", "perdu", "lost", "plante", "crash"],
        Some("Ctrl + Maj + R")),
    tool("add_video", Project, "Add a video to the project (importer une vidéo, ajouter une vidéo).", &[],
        &["ajoute une video", "ajouter une video", "importe une video", "importer une video", "charge une video", "add a video", "import a video", "load a video", "agrega un video", "anade un video", "nouvelle video", "charge la video", "ouvre la video", "load the video"],
        Some("Menu Projet > Ajouter une vidéo")),
    tool("add_audio", Project, "Add an audio file to the project (ajouter un audio, piste audio, musique).", &[],
        &["ajoute un audio", "ajouter un audio", "importe un audio", "importer un audio", "ajoute une musique", "ajoute un son", "add an audio", "import an audio", "add audio", "agrega un audio", "fichier audio", "fichier wav", "fichier mp3"],
        Some("Menu Projet > Ajouter un audio")),
    tool("set_instrumental_audio", Project, "Choose the instrumental audio file of the band (audio instrumental, version sans voix).", &[],
        &["audio instrumental", "piste instrumentale", "version instrumentale", "sans voix", "sans les voix", "musique seule", "m e ", "international", "choisis l audio", "choisir l audio", "instrumental audio"],
        Some("Ctrl + O > Audio instrumental")),
    tool("add_band", Project, "Create a new rythmo band, a language version, with the name said in the query (nouvelle bande, bande anglaise).",
        &[opt("name", Text, "Band name, such as a language, if said")],
        &["nouvelle bande", "nouvelle langue", "nouvelle version", "cree une bande", "creer une bande", "ajoute une bande", "ajouter une bande", "ajoute une langue", "new band", "add a band", "new language", "nueva banda", "crea una banda", "traduction", "translation"],
        Some("Fichiers du projet (Ctrl + L)")),
    tool("select_band", Project, "Show another rythmo band, the one said in the query (affiche la bande anglaise, passe en anglais).",
        &[req("name", Text, "Band name")],
        &["affiche la bande", "montre la bande", "passe a la bande", "passe en", "change de bande", "switch band", "switch to the band", "show the band", "cambia a la banda", "bande anglaise", "bande francaise", "bande espagnole", "english band", "french band", "va sur la bande", "switch to", "band", "la banda"],
        Some("Fichiers du projet (Ctrl + L)")),
    tool("rename_band", Project, "Rename the current rythmo band with the name said in the query (renomme la bande).",
        &[req("name", Text, "New name")],
        &["renomme la bande", "renommer la bande", "rename the band", "rename band", "renombra la banda", "nom de la bande", "appelle la bande"],
        Some("Fichiers du projet (Ctrl + L) > F2")),
    tool("delete_band", Project, "Delete the rythmo band said in the query (supprime la bande anglaise).",
        &[req("name", Text, "Band name")],
        &["supprime la bande", "supprimer la bande", "efface la bande", "delete the band", "delete band", "remove the band", "elimina la banda", "borra la banda"],
        Some("Fichiers du projet (Ctrl + L) > Suppr")),
    // ---- Conform (new cut of the video) ---------------------------------
    tool("open_conform", Open, "Open « Ajouter et conformer »: update the project to a new cut of its video, a V2 or V3 of the edit (conformer, nouveau montage, nouvelle version). The window asks for the project video, the band and the new file.",
        &[opt("video", Text, "Project video name, if said"), opt("band", Text, "Band name, if said")],
        &["conforme", "conformer", "conformation", "conform", "conforma", "conformar", "nouveau montage", "nouvelle version du montage", "nouvelle version de la video", "nouvelle version", "new cut", "new edit", "new version", "nuevo montaje", "nueva version", "v2", "v3", "version 2", "version 3", "remontage", "recut", "ajouter et conformer", "add and conform"],
        Some("Ctrl + L > clic droit sur Vidéos > Ajouter et conformer")),
    tool("conform_video", Edit, "Compare the project video with the new cut whose file path is written in the query, then move dialogues, detections, loops, drawings and takes to it and report what changed (conforme avec le fichier C:\\...\\V2.mp4).",
        &[req("path", Text, "Path of the new cut on the disk, exactly as written"), opt("video", Text, "Project video name, if said"), opt("band", Text, "Band name, if said")],
        &["conforme avec", "conformer avec", "conforme sur", "conform with", "conform to", "conforma con", "chemin", "path", "fichier mp4", "fichier mov"],
        Some("Ctrl + L > clic droit sur Vidéos > Ajouter et conformer")),
    tool("get_conform_report", Info, "Tell what the last conformation moved, modified, deleted or marked to record again (rapport de conformation, qu'est-ce qui a bougé, à réenregistrer).", &[],
        &["rapport de conformation", "rapport de la conformation", "resultat de la conformation", "conformation report", "conform report", "informe de conformacion", "qu est ce qui a bouge", "what moved", "qu est ce qui a change", "what changed", "a reenregistrer", "to re record", "to rerecord", "rerecord", "reenregistrer", "reenregistre", "regrabar", "quelles repliques ont bouge"],
        None),
    tool("list_bands", Info, "Tell which rythmo bands (languages) the project has and which one is shown (quelles bandes, combien de langues).", &[],
        &["liste des bandes", "liste les bandes", "quelles bandes", "quelles langues", "combien de bandes", "combien de langues", "which bands", "which languages", "how many bands", "list the bands", "cuantas bandas", "quelle bande est affichee", "which band is shown", "les bandes", "les langues"],
        Some("Ctrl + L")),
    // ---- Information ----------------------------------------------------
    tool("get_project_info", Info, "Tell the project name, video, duration, frame rate, line count (infos du projet, quel projet, quelle durée).", &[],
        &["info", "infos", "le projet", "quel projet", "which project", "proyecto", "duree de la video", "duree du film", "combien de temps dure", "how long", "duration", "fps", "images par seconde", "frame rate", "quel fichier", "which file", "nom du projet", "project name", "resume", "summary", "etat", "status", "cadence"],
        None),
    tool("count_lines", Info, "Count the dialogue lines, all of them or those of the character said in the query (combien de répliques a Jean).",
        &[opt("character", Text, CHARACTER)],
        &["combien de repliques", "combien de lignes", "combien de dialogues", "nombre de repliques", "nombre de lignes", "how many lines", "number of lines", "cuantas lineas", "cuantas replicas", "compte les repliques", "compte les lignes", "count the lines", "statistiques", "stats", "cuantas"],
        None),
    tool("find_line", Info, "Find the dialogue line containing the text said in the query, select it and go to it (cherche la réplique qui dit, trouve, où est).",
        &[req("text", Text, "Text to find, as quoted in the query")],
        &["cherche", "chercher", "trouve", "trouver", "find", "search", "busca", "buscar", "ou est la replique", "ou est la ligne", "where is the line", "donde esta", "localise", "retrouve", "la replique qui dit", "la ligne qui dit", "the line that says", "qui dit"],
        Some("Ctrl + I puis taper le texte")),
    tool("get_playhead_time", Info, "Tell the current timecode of the playhead (où suis-je, quel timecode, quelle position).", &[],
        &["ou suis je", "ou en suis je", "ou en est on", "where am i", "quel timecode", "quelle position", "quel temps", "what time", "le timecode", "la position", "position actuelle", "current position", "quelle minute", "a quel moment", "donde estoy", "quelle image", "current frame", "image courante"],
        Some("Affiché dans la barre d'outils")),
    tool("get_selected_line", Info, "Tell the text, character and timing of the selected line (quelle réplique est sélectionnée, lis-moi la ligne).", &[],
        &["quelle replique", "quelle ligne", "which line", "ligne selectionnee", "replique selectionnee", "selected line", "que dit la ligne", "que dit la replique", "what does the line say", "lis moi la ligne", "lis moi la replique", "read me the line", "linea seleccionada", "la ligne actuelle", "la replique actuelle"],
        None),
    fallback_tool("get_shortcut", Info, "Tell the keyboard shortcut of a feature or where to find it in the menus (raccourci, comment faire, où est).",
        &[req("feature", Text, "Feature or action wanted, in the user's words")],
        &["raccourci", "shortcut", "atajo", "quel est le raccourci", "c est quoi le raccourci", "le raccourci pour", "what is the shortcut", "shortcut for", "cual es el atajo", "touche", "quelle touche", "which key", "comment on", "comment je", "comment faire", "how do i", "how to", "ou est", "ou se trouve", "ou trouver", "where is", "where do i", "oublie", "forgot", "forgotten", "souviens", "remember", "dans quel menu", "which menu", "como se", "c est quoi la touche", "quelle est la touche", "la touche pour", "j ai oublie comment", "comment on fait pour"]),
    fallback_tool("help", Info, "Explain what the assistant can do (aide, help, que sais-tu faire).",
        &[],
        // Greetings are not triggers: "Bonjour" is more often dialogue text.
        &["aide", "help", "ayuda", "que sais tu", "que peux tu", "what can you", "capable", "a quoi tu sers", "qui es tu", "who are you"]),
    // ---- Collaboration --------------------------------------------------
    tool("open_server_browser", Open, "Open the online collaboration servers list (serveurs, en ligne, collaboration, rejoindre).", &[],
        &["serveur", "server", "servidor", "en ligne", "online", "collabor", "rejoins", "join", "connecte", "connect", "session en ligne", "salle", "room"],
        Some("Menu Collaboration")),
    tool("copy_invitation_link", Project, "Copy the link inviting someone to the current online session (inviter, lien d'invitation).", &[],
        &["invit", "lien d invitation", "copie le lien", "copy the link", "lien de la session", "session link", "enlace de invitacion", "partage la session", "share the session"],
        Some("Menu Collaboration > Inviter")),
    tool("disconnect", Project, "Leave the online session (déconnecter, quitter la session).", &[],
        &["deconnect", "disconnect", "desconect", "quitte la session", "leave the session", "hors ligne", "offline"],
        Some("Menu Collaboration > Se déconnecter")),
    // ---- Accessibility --------------------------------------------------
    tool("toggle_screen_reader", Open, "Turn the internal voice reading on or off (lecture vocale, lecteur d'écran, narration).", &[],
        &["lecture vocale", "lecteur d ecran", "screen reader", "narration", "voix de synthese", "parle moi", "read aloud", "lector de pantalla", "accessibilit", "synthese vocale"],
        Some("Ctrl + Maj + N")),
];

pub fn find(name: &str) -> Option<&'static ToolSpec> {
    CATALOG.iter().find(|tool| tool.name == name)
}

/// The JSON function schemas of these tools, as the engine reads them.
pub fn schema_json(tools: &[&ToolSpec]) -> String {
    let schemas: Vec<serde_json::Value> = tools.iter().map(|tool| schema(tool)).collect();
    serde_json::to_string(&schemas).unwrap_or_else(|_| "[]".to_string())
}

pub fn schema(tool: &ToolSpec) -> serde_json::Value {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();
    for param in tool.params {
        let mut property = serde_json::Map::new();
        let kind = match param.kind {
            ParamKind::Text => "string",
            ParamKind::Number => "number",
            ParamKind::Integer => "integer",
            ParamKind::Bool => "boolean",
            ParamKind::Choice(_) => "string",
        };
        property.insert("type".into(), kind.into());
        if let ParamKind::Choice(choices) = param.kind {
            property.insert("enum".into(), serde_json::json!(choices));
        }
        property.insert("description".into(), param.description.into());
        properties.insert(param.name.to_string(), serde_json::Value::Object(property));
        if param.required {
            required.push(param.name);
        }
    }
    serde_json::json!({
        "name": tool.name,
        "description": tool.description,
        "parameters": {
            "type": "object",
            "properties": properties,
            "required": required,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique_snake_case() {
        let mut seen = std::collections::HashSet::new();
        for tool in CATALOG {
            assert!(seen.insert(tool.name), "duplicate tool {}", tool.name);
            assert!(
                tool.name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '_' || c.is_ascii_digit()),
                "{} is not snake_case",
                tool.name
            );
            assert!(!tool.description.is_empty());
            assert!(!tool.triggers.is_empty(), "{} has no trigger", tool.name);
        }
    }

    #[test]
    fn triggers_are_normalized_stems() {
        for tool in CATALOG {
            for trigger in tool.triggers {
                let normalized = super::super::retrieval::normalize(trigger);
                assert_eq!(
                    normalized.trim_start(),
                    format!("{}{}", trigger, if trigger.ends_with(' ') { "" } else { " " }),
                    "trigger {trigger:?} of {} is not lower-case ASCII",
                    tool.name
                );
            }
            for param in tool.params {
                assert!(
                    param.name.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                    "{}.{}",
                    tool.name,
                    param.name
                );
            }
        }
    }

    #[test]
    fn schema_is_the_engine_format() {
        let json = schema_json(&[find("seek_to_time").unwrap()]);
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value[0]["name"], "seek_to_time");
        assert_eq!(value[0]["parameters"]["type"], "object");
        assert_eq!(value[0]["parameters"]["required"][0], "time");
        assert_eq!(value[0]["parameters"]["properties"]["time"]["type"], "string");
        let breath = schema(find("add_breath").unwrap());
        assert_eq!(breath["parameters"]["properties"]["kind"]["enum"][0], "h");
    }

    #[test]
    fn every_tool_has_an_i18n_label() {
        for tool in CATALOG {
            let key = format!("assistant.tool.{}", tool.name);
            assert_ne!(crate::i18n::t(&key), key.as_str(), "missing {key}");
        }
    }
}
