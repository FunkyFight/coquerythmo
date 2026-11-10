"""Training queries for Needle 3, the assistant of the Bande rythmo.

Writes one JSON object per line: the user's sentence, the session facts the
app would send, and the exact calls expected, arguments copied verbatim from
the sentence. The Rust exporter (`cargo test --lib ai::training -- --ignored`)
then picks the tools the app would really send for each sentence and writes
the final training file, so the model learns on what it will see.

Usage: python generate_queries.py OUT.jsonl [--per-template N] [--seed S]
"""

import argparse
import json
import random
import unicodedata

CHARACTERS = [
    "Marie", "Jean", "Muffins", "Cup Cakes", "Twilight", "Rarity", "Pinkie Pie", "Le Narrateur",
    "Docteur Martin", "Zoé", "Hélène", "Bob", "Mister Smith", "La Reine", "Chanson", "Capitaine",
    "Lucas", "Emma", "Postière", "Sergent Garcia", "Applejack", "Spike", "Maman", "Le Chef",
]
TEXTS = [
    "Bonjour tout le monde", "Hello World", "Je reviens tout de suite", "Où est-ce que tu vas ?",
    "Attends-moi !", "C'est pas possible", "On y va", "Merci beaucoup", "Tu m'as manqué",
    "Non, non et non !", "Postière Muffins", "I'll be back", "Let's go", "Qué pasa", "Je t'aime",
    "Au secours !", "Ha ha ha", "Bien joué", "Silence", "Viens ici", "Ça suffit maintenant",
    "Regarde là-haut", "Il est parti", "Encore toi ?", "Quelle journée",
]
TIMES = [
    "1:30", "1 min 30", "2 minutes", "00:01:30:12", "90s", "45 secondes", "3:05", "1 minute 30",
    "2 min 15", "0:45", "12:10", "5 secondes", "00:00:10:00", "2:00", "3 minutes 20", "l'image 120",
    "10 secondes", "4:12", "1h02", "30s",
]
COLORS = ["rouge", "bleu", "vert", "jaune", "orange", "violet", "rose", "noir", "blanc", "gris", "red", "blue",
          "green", "#ff8800", "turquoise", "marron", "bleu clair", "pink"]
BANDS = ["anglais", "français", "espagnol", "EN", "FR", "allemand", "English", "italien", "japonais"]
NEW_NAMES = ["Pierre", "Cup Cakes", "Cupcake", "Le Roi", "Mademoiselle", "Sunny", "Docteur Who", "Tom", "Lily"]
ACTORS = ["Paul", "Sophie Martin", "Kevin", "Julie", "Thomas Durand", "Ana", "Marc Lavoine"]
EMOTIONS = ["colère", "joie", "peur", "tristesse", "doute", "question", "exclamation", "tremblement", "vague",
            "rebond", "colère extrême", "un peu de joie", "grosse peur", "question rapide", "shake", "amour",
            "dégoût", "panique"]
NOTES = ["à refaire", "trop rapide", "vérifier la synchro", "respiration avant", "plus fort", "OK validé",
         "retake demain", "attention à la liaison"]
FEATURES = ["exporter", "créer une boucle", "mettre en gras", "ouvrir le style de la bande",
            "renommer un personnage", "sauvegarder", "scinder une réplique", "mettre en karaoké",
            "ouvrir les fichiers du projet", "ajouter une respiration", "importer des sous-titres"]
FINDS = ["bonjour", "je reviens", "merci", "au secours", "on y va", "tu m'as manqué"]
SECONDS = ["2", "1.5", "3", "0.5", "10", "5", "1"]
PERCENTS = ["30", "50", "80", "20", "100", "0", "75", "10"]
MILLIS = ["200", "-150", "500", "80", "-40", "1000"]
SPEEDS = ["1.5", "2", "0.75", "1", "1.25", "3"]
HEIGHTS = ["80", "120", "60", "40", "150"]
OFFSETS = ["10", "-20", "15", "0", "25"]
COUNTS = ["5", "10", "3", "24", "-5", "-10", "2"]
TRACKS = ["1", "2", "3", "4"]
START_TCS = ["01:00:00:00", "10:00:00:00", "00:00:00:00", "09:59:50:00"]
BREATHS = [("H", "h"), ("HH", "hh"), ("mH", "mh"), ("mHH", "mhh")]
REACTIONS = ["ah", "oh", "pff", "tsc", "mhm", "psst", "hm"]

SLOTS = {
    "C": CHARACTERS, "C2": CHARACTERS, "T": TEXTS, "TIME": TIMES, "COL": COLORS, "B": BANDS,
    "NEW": NEW_NAMES, "A": ACTORS, "E": EMOTIONS, "NOTE": NOTES, "FEAT": FEATURES, "FIND": FINDS,
    "SEC": SECONDS, "PCT": PERCENTS, "MS": MILLIS, "SPEED": SPEEDS, "PX": HEIGHTS, "OFF": OFFSETS,
    "N": COUNTS, "TR": TRACKS, "TC": START_TCS, "R": REACTIONS,
}

# tool: (arguments as "param=SLOT" pairs, templates). A template's slots give
# the arguments it carries; a slot absent from a template means the argument
# is not said, so it is not in the expected call.
TOOLS = {
    # ---- playback
    "play": ("", ["lance la lecture", "lis la vidéo", "joue la vidéo", "démarre la lecture", "reprends la lecture",
                   "play", "lecture", "start playback", "play the video", "reproduce el vídeo", "lance la vidéo"]),
    "pause": ("", ["pause", "mets en pause", "stop", "arrête la lecture", "stoppe la vidéo", "pause the video",
                    "fige la vidéo", "para el vídeo", "arrête tout"]),
    "play_reverse": ("", ["lis à l'envers", "lecture inversée", "joue la vidéo à l'envers", "play backwards",
                           "lecture en arrière", "rembobine en lisant"]),
    "seek_to_time": ("time=TIME", ["va à {TIME}", "aller à {TIME}", "place-toi à {TIME}", "positionne-toi à {TIME}",
                                   "mets la tête de lecture à {TIME}", "go to {TIME}", "jump to {TIME}", "ve a {TIME}",
                                   "Va a {TIME}", "rends-toi à {TIME}", "saute à {TIME}", "va au timecode {TIME}"]),
    "seek_relative": ("seconds=SEC", ["avance de {SEC} secondes", "recule de {SEC} secondes", "skip {SEC} seconds",
                                      "avance de {SEC} s", "go forward {SEC} seconds", "retrocede {SEC} segundos"]),
    "next_frame": ("", ["image suivante", "avance d'une image", "next frame", "frame suivante", "fotograma siguiente",
                         "une image plus loin"]),
    "previous_frame": ("", ["image précédente", "recule d'une image", "previous frame", "frame précédente",
                             "fotograma anterior", "l'image d'avant"]),
    "step_frames": ("count=N", ["avance de {N} images", "recule de {N} images", "move {N} frames",
                                "avance de {N} frames", "avanza {N} fotogramas"]),
    "go_to_start": ("", ["va au début", "retour au début", "go to the beginning", "tout au début", "ve al inicio",
                          "reviens au début de la vidéo"]),
    "go_to_end": ("", ["va à la fin", "tout à la fin", "go to the end", "va au bout de la vidéo", "ve al final"]),
    "go_to_next_loop": ("", ["boucle suivante", "va à la boucle suivante", "prochaine boucle", "next loop",
                              "saute à la boucle suivante", "bucle siguiente"]),
    "go_to_previous_loop": ("", ["boucle précédente", "reviens à la boucle précédente", "previous loop",
                                  "la boucle d'avant", "bucle anterior"]),
    "set_volume": ("percent=PCT", ["mets le volume à {PCT}", "volume à {PCT}", "baisse le son à {PCT}",
                                   "monte le son à {PCT} %", "set the volume to {PCT}", "son à {PCT} pour cent",
                                   "pon el volumen a {PCT}"]),
    "volume_up": ("", ["monte le son", "plus fort", "louder", "augmente le volume", "monte le volume",
                        "sube el volumen", "on n'entend rien, plus fort"]),
    "volume_down": ("", ["baisse le son", "moins fort", "quieter", "baisse le volume", "diminue le son",
                          "baja el volumen", "c'est trop fort, baisse le son"]),
    "mute": ("", ["coupe le son", "mute", "silence", "muet", "mets en sourdine", "enlève le son", "silencia"]),
    "unmute": ("", ["remets le son", "unmute", "réactive le son", "remet le son", "activa el sonido"]),
    "switch_audio": ("", ["bascule sur l'instrumental", "passe en VO", "audio original", "switch audio",
                           "mets la version instrumentale", "bascule l'audio"]),
    "offset_audio": ("milliseconds=MS", ["décale l'audio de {MS} ms", "offset audio {MS} millisecondes",
                                        "décale le son de {MS} ms", "audio offset {MS} ms"]),
    "open_secondary_display": ("", ["ouvre la fenêtre secondaire", "affiche le deuxième écran",
                                     "open the second screen", "ouvre l'écran des comédiens"]),
    "close_secondary_display": ("", ["ferme la fenêtre secondaire", "ferme le deuxième écran",
                                      "close the secondary window"]),
    # ---- selection
    "select_line_at_playhead": ("", ["sélectionne la ligne ici", "sélectionne la réplique sous le curseur",
                                      "select the line at the playhead", "la ligne courante"]),
    "select_next_line": ("", ["réplique suivante", "ligne suivante", "next line", "passe à la suivante",
                               "línea siguiente"]),
    "select_previous_line": ("", ["réplique précédente", "ligne précédente", "previous line",
                                   "reviens à la précédente", "línea anterior"]),
    "select_all": ("", ["tout sélectionner", "sélectionne tout", "sélectionne toutes les lignes", "select all",
                         "selecciona todo"]),
    "select_lines": ("character=C;text=FIND", ["sélectionne les répliques de {C}", "sélectionne les lignes de {C}",
                                               "select the lines of {C}", "selecciona las líneas de {C}",
                                               "sélectionne les répliques contenant {FIND}"]),
    "clear_selection": ("", ["désélectionne", "annule la sélection", "deselect", "enlève la sélection",
                              "plus rien de sélectionné"]),
    # ---- lines
    "create_line": ("text=T;character=C;track=TR", [
        "crée une réplique de {C} : {T}", "crée une réplique de {C} sur la piste {TR} : {T}",
        "Créé une phrase piste {TR} qui dit \"{T}\"", "Créé une phrase ligne {TR} qui dit \"{T}\"",
        "nouvelle réplique de {C}", "ajoute une ligne pour {C} qui dit « {T} »", "crée une réplique",
        "nouvelle ligne sur la piste {TR}", "écris \"{T}\" sur la piste {TR}", "create a line for {C}: {T}",
        "new line on track {TR}", "crea una línea de {C}: {T}", "ajoute une réplique : {T}",
        "{C} dit \"{T}\", crée la réplique", "nouvelle phrase \"{T}\""]),
    "set_line_text": ("text=T", ["change le texte en \"{T}\"", "remplace le texte par {T}",
                                 "le texte de la ligne doit être « {T} »", "change the text to \"{T}\"",
                                 "cambia el texto a \"{T}\"", "corrige le texte : {T}"]),
    "edit_line_text": ("", ["modifie le texte", "édite le texte de la ligne", "edit the text",
                             "ouvre le texte de la réplique", "je veux corriger le texte"]),
    "edit_line_sections": ("", ["modifie la ligne section par section", "édition par section",
                                 "modifie le texte sans perdre le calage"]),
    "set_line_character": ("character=C", ["attribue la ligne à {C}", "c'est {C} qui parle", "mets le personnage {C}",
                                           "donne le personnage {C} à la réplique", "assign {C} to the line",
                                           "this line is said by {C}", "asigna {C}", "le personnage à {C}"]),
    "edit_line_character": ("", ["modifie le personnage", "change le personnage de la ligne",
                                  "edit the character", "ouvre le champ personnage"]),
    "delete_selected_lines": ("", ["supprime la sélection", "supprime la ligne", "efface cette réplique",
                                    "delete the selected lines", "borra la línea", "vire cette ligne"]),
    "delete_lines": ("character=C;text=FIND", ["supprime toutes les répliques de {C}", "efface les lignes de {C}",
                                               "delete all the lines of {C}", "borra las líneas de {C}",
                                               "supprime les répliques contenant {FIND}"]),
    "copy_lines": ("", ["copie la ligne", "copie", "copy", "copie les répliques", "duplique la sélection"]),
    "cut_lines": ("", ["coupe la ligne", "cut", "coupe les répliques", "corta la línea"]),
    "paste_lines": ("", ["colle", "colle les lignes", "paste", "pega", "colle ici"]),
    "paste_lines_with_track_character": ("", ["colle avec le personnage de la piste",
                                               "paste with the character of the track"]),
    "move_lines_to_track": ("track=TR", ["déplace la réplique sur la piste {TR}", "mets la ligne sur la piste {TR}",
                                         "move it to track {TR}", "change de piste, piste {TR}", "sur la piste {TR}",
                                         "mueve la línea a la pista {TR}"]),
    "move_lines_to_time": ("time=TIME", ["positionne-la à {TIME}", "place la ligne à {TIME}", "mets-la à {TIME}",
                                         "move it to {TIME}", "déplace la réplique au timecode {TIME}",
                                         "colócala en {TIME}"]),
    "move_lines_to_character_line": ("character=C", [
        "positionne-la au niveau de la première ligne de {C}", "place-la en face de la réplique de {C}",
        "aligne-la sur la dernière réplique de {C}", "move it to the same place as the line of {C}",
        "Positionne là au niveau de la première ligne de {C}"]),
    "shift_lines": ("seconds=SEC", ["décale la ligne de {SEC} secondes", "décale de {SEC} s vers la droite",
                                    "shift the line {SEC} seconds later", "pousse la réplique de {SEC} secondes"]),
    "set_line_start": ("", ["début ici", "fixe le début de la ligne ici", "la réplique commence ici",
                             "set the start here", "point d'entrée ici"]),
    "set_line_end": ("", ["fin ici", "fixe la fin de la ligne ici", "la réplique finit ici", "set the end here",
                           "point de sortie ici"]),
    "set_line_duration": ("seconds=SEC", ["la ligne dure {SEC} secondes", "durée de {SEC} secondes",
                                          "set the duration to {SEC} seconds", "allonge la réplique à {SEC} s"]),
    "split_line": ("", ["scinde la réplique", "coupe la ligne en deux", "split the line", "sépare la réplique",
                         "divide la línea"]),
    "toggle_karaoke": ("", ["mets en karaoké", "active le karaoké", "karaoké", "enlève le karaoké", "karaoke"]),
    "toggle_bold": ("", ["mets en gras", "gras", "mets la ligne en gras", "bold", "en negrita",
                          "Met la TOUTE PREMIERE LIGNE en gras", "mets la dernière réplique en gras"]),
    "toggle_italic": ("", ["mets en italique", "italique", "italic", "en cursiva", "mets la réplique en italique"]),
    "toggle_underline": ("", ["souligne", "souligne la ligne", "underline", "subraya el texto"]),
    "toggle_strikethrough": ("", ["barre le texte", "barré", "strikethrough", "raye la réplique"]),
    "set_text_emotion": ("emotion=E", ["mets de la {E}", "émotion {E}", "anime la ligne avec {E}",
                                       "ajoute l'émotion {E}", "{E} sur la réplique", "make it {E}"]),
    "clear_text_emotion": ("", ["enlève l'émotion", "sans émotion", "retire l'effet", "remove the emotion",
                                 "plus d'émotion"]),
    "open_emotion_menu": ("", ["ouvre le menu des émotions", "menu émotion", "liste des émotions"]),
    "link_lines": ("", ["lie ces dialogues", "lie les répliques", "link these lines", "dialogues liés"]),
    "link_lines_with_character": ("", ["lie avec le personnage", "link with the character"]),
    "unlink_lines": ("", ["délie", "délie les dialogues", "unlink", "détache les répliques"]),
    "set_line_note": ("note=NOTE", ["note : {NOTE}", "mets une note : {NOTE}", "commentaire : {NOTE}",
                                    "écris en note {NOTE}", "comment: {NOTE}"]),
    "add_note": ("", ["ajoute une note", "nouvelle note", "add a note", "ouvre la note"]),
    "set_line_on_screen": ("", ["dans le champ", "à l'écran", "on screen", "on le voit", "en plano"]),
    "set_line_off_screen": ("", ["hors champ", "voix off", "off screen", "en off", "fuera de plano"]),
    "set_line_back": ("", ["de dos", "il est de dos", "from the back", "de espaldas"]),
    "generate_detection_signs": ("", ["génère les signes de détection", "convertis en signes",
                                       "generate the detection signs"]),
    "undo": ("", ["annule", "undo", "oups, annule", "retour en arrière", "deshacer", "défais ça"]),
    "redo": ("", ["rétablis", "refais", "redo", "rehacer"]),
    # ---- markers
    "add_loop": ("", ["ajoute une boucle", "pose une boucle", "nouvelle boucle", "add a loop", "boucle ici",
                       "añade un bucle", "mets une boucle"]),
    "add_out_point": ("", ["ajoute un point out", "point out ici", "add an out point", "pose un out"]),
    "add_scene_change": ("", ["ajoute un changement de plan", "changement de plan ici", "add a scene change",
                               "nouveau plan", "cambio de plano"]),
    "add_liaison_left": ("", ["liaison à gauche", "ajoute une liaison gauche", "left liaison"]),
    "add_liaison_right": ("", ["liaison à droite", "ajoute une liaison droite", "right liaison"]),
    "add_breath": ("kind=BR", ["ajoute une respiration", "respiration", "add a breath", "souffle ici",
                               "ajoute une respiration {BR}"]),
    "add_reaction": ("kind=R", ["ajoute une réaction", "réaction ici", "add a reaction", "ajoute un rire",
                                "ajoute une réaction {R}"]),
    "add_sync_point": ("", ["ajoute un point de synchro", "point de synchro ici", "add a sync point",
                             "pose une ancre de synchro"]),
    "list_markers": ("", ["liste les boucles", "combien de boucles ?", "où sont les boucles ?",
                           "list the markers", "combien de marqueurs"]),
    # ---- characters
    "rename_character": ("old_name=C;new_name=NEW", [
        "renomme {C} en {NEW}", "Renomme le personnage \"{C}\" en \"{NEW}\"", "{C} s'appelle maintenant {NEW}",
        "rename {C} to {NEW}", "renombra {C} a {NEW}", "change le nom de {C} en {NEW}", "{C} devient {NEW}"]),
    "set_character_color": ("character=C;color=COL", ["mets {C} en {COL}", "couleur de {C} : {COL}",
                                                      "change la couleur de {C} en {COL}",
                                                      "make {C} {COL}", "{C} en {COL}"]),
    "list_characters": ("", ["liste les personnages", "quels sont les personnages ?", "qui parle dans le projet ?",
                              "list the characters", "combien de personnages ?"]),
    "open_roles_panel": ("", ["ouvre le panneau des rôles", "ouvre les rôles", "roles panel",
                               "affiche le tableau des rôles"]),
    "open_lines_panel": ("", ["ouvre le tableau des lignes", "panneau des lignes", "liste des répliques",
                               "open the lines panel", "montre le script"]),
    "open_rename_character": ("", ["ouvre la fenêtre renommer", "ouvre renommer un personnage"]),
    "open_voice_actors": ("", ["ouvre les comédiens", "fenêtre des comédiens", "voice actors", "casting"]),
    "create_voice_actor": ("name=A", ["crée un comédien {A}", "nouveau comédien : {A}", "create a voice actor {A}",
                                      "ajoute un comédien {A}"]),
    "assign_voice_actor_to_line": ("actor=A", ["attribue le comédien {A} à la ligne", "la réplique est doublée par {A}",
                                               "assign the actor {A} to the line"]),
    "assign_voice_actor_to_character": ("actor=A", ["{A} double tout le personnage",
                                                    "attribue {A} au personnage",
                                                    "assign {A} to the whole character"]),
    # ---- drawing
    "use_select_tool": ("", ["outil sélection", "passe en mode sélection", "selection tool", "arrête de dessiner"]),
    "use_draw_tool": ("", ["outil dessin", "passe en mode dessin", "draw tool", "je veux dessiner", "pinceau"]),
    "toggle_eraser": ("", ["gomme", "prends la gomme", "eraser", "efface les traits"]),
    "change_brush_size": ("", ["taille du pinceau", "pinceau plus gros", "brush size", "trait plus fin"]),
    "open_brush_color": ("", ["couleur du pinceau", "change la couleur du trait", "brush color"]),
    # ---- export, windows
    "open_export": ("", ["exporte", "ouvre l'export", "exporter la vidéo", "export", "exporte la vidéo",
                          "fais le rendu", "rendu mp4", "exporta"]),
    "export_subtitles": ("", ["exporte les sous-titres", "exporte en srt", "export the subtitles",
                               "exporte le script en pdf", "exporta los subtítulos"]),
    "cancel_export": ("", ["annule l'export", "arrête l'export", "cancel the export", "stoppe l'export"]),
    "open_proxy": ("", ["crée un proxy", "ouvre le proxy", "vidéo légère", "create a proxy"]),
    "open_project_settings": ("", ["ouvre les paramètres du projet", "paramètres du projet", "project settings",
                                    "réglages du projet"]),
    "set_scroll_speed": ("value=SPEED", ["vitesse de défilement à {SPEED}", "mets la vitesse de défilement à {SPEED}",
                                         "scroll speed {SPEED}"]),
    "set_reading_bar_offset": ("value=OFF", ["barre de lecture à {OFF} %", "décale la barre de lecture à {OFF}",
                                             "reading bar {OFF} percent"]),
    "toggle_highlight_read_word": ("", ["surligne le mot lu", "active le surlignage du mot lu",
                                         "highlight the word"]),
    "toggle_character_color_text": ("", ["texte à la couleur du personnage",
                                          "affiche le texte en couleur du personnage"]),
    "toggle_emotion_lanes": ("", ["affiche les pistes d'émotions", "masque les pistes d'émotion",
                                   "emotion lanes"]),
    "toggle_timecode_display": ("", ["affiche le timecode", "masque le timecode", "show the timecode"]),
    "set_start_timecode": ("value=TC", ["timecode de départ {TC}", "le timecode de début est {TC}",
                                        "start timecode {TC}"]),
    "set_waveform_height": ("value=PX", ["forme d'onde à {PX} px", "hauteur de la forme d'onde {PX}",
                                         "waveform height {PX}"]),
    "open_band_style": ("", ["ouvre le style de la bande", "style de la bande", "band style",
                              "change l'apparence de la bande", "presets de style"]),
    "open_settings": ("", ["ouvre les préférences", "paramètres de l'application", "change la langue",
                            "app settings"]),
    "show_shortcuts": ("", ["affiche les raccourcis", "montre les raccourcis clavier", "show the shortcuts"]),
    "hide_shortcuts": ("", ["masque les raccourcis", "cache les raccourcis", "hide the shortcuts"]),
    "open_automation": ("", ["ouvre l'automatisation", "graphe d'automatisation", "automation"]),
    "toggle_project_files": ("", ["ouvre les fichiers du projet", "explorateur", "project files",
                                   "ferme les fichiers du projet"]),
    "open_recent_projects": ("", ["projets récents", "ouvre les derniers projets", "recent projects"]),
    "close_panel": ("", ["ferme le panneau", "ferme la fenêtre", "close the panel", "ferme tout"]),
    # ---- project
    "save_project": ("", ["sauvegarde", "sauvegarde le projet", "enregistre", "save", "guarda el proyecto",
                           "enregistre le projet", "sauve"]),
    "save_project_as": ("", ["enregistre sous", "sauvegarde sous un autre nom", "save as", "guardar como"]),
    "new_project": ("", ["nouveau projet", "crée un nouveau projet", "new project", "nuevo proyecto"]),
    "open_project": ("", ["ouvre un projet", "charge un projet", "open a project", "abre un proyecto"]),
    "close_project": ("", ["ferme le projet", "close the project", "cierra el proyecto"]),
    "import_subtitles": ("", ["importe des sous-titres", "importe un fichier srt", "import subtitles",
                               "charge un sous-titrage"]),
    "restore_backup": ("", ["restaure la sauvegarde", "récupère la dernière sauvegarde", "restore the backup",
                             "l'app a planté, restaure"]),
    "add_video": ("", ["ajoute une vidéo", "importe une vidéo", "add a video", "charge la vidéo"]),
    "add_audio": ("", ["ajoute un audio", "importe un fichier audio", "add an audio file", "ajoute une musique"]),
    "set_instrumental_audio": ("", ["choisis l'audio instrumental", "piste instrumentale",
                                     "instrumental audio"]),
    "add_band": ("name=B", ["nouvelle bande", "crée une bande {B}", "ajoute une langue {B}", "new band {B}",
                            "crée une nouvelle version"]),
    "select_band": ("name=B", ["passe en {B}", "affiche la bande {B}", "switch to the {B} band",
                               "montre la bande {B}", "cambia a la banda {B}"]),
    "rename_band": ("name=B", ["renomme la bande en {B}", "rename the band to {B}"]),
    "delete_band": ("name=B", ["supprime la bande {B}", "delete the band {B}", "efface la bande {B}"]),
    "list_bands": ("", ["quelles bandes ?", "liste les bandes", "combien de langues ?", "which bands"]),
    # ---- information
    "get_project_info": ("", ["infos du projet", "quel projet est ouvert ?", "quelle est la durée de la vidéo ?",
                               "project info", "résumé du projet"]),
    "count_lines": ("character=C", ["combien de répliques a {C} ?", "combien de lignes pour {C}",
                                    "combien de répliques ?", "how many lines does {C} have?",
                                    "how many lines?", "cuántas líneas tiene {C}"]),
    "find_line": ("text=FIND", ["cherche la réplique qui dit {FIND}", "trouve la ligne {FIND}",
                                "où est la réplique \"{FIND}\" ?", "find the line that says {FIND}",
                                "busca la línea {FIND}"]),
    "get_playhead_time": ("", ["où suis-je ?", "quel timecode ?", "c'est quoi le timecode actuel",
                                "where am I?", "quelle position ?"]),
    "get_selected_line": ("", ["quelle ligne est sélectionnée ?", "lis-moi la réplique", "which line is selected?",
                                "que dit la ligne ?"]),
    "get_shortcut": ("feature=FEAT", ["quel est le raccourci pour {FEAT} ?", "comment on fait pour {FEAT} ?",
                                      "c'est quoi la touche pour {FEAT}", "what is the shortcut to {FEAT}?",
                                      "j'ai oublié comment {FEAT}"]),
    "help": ("", ["aide", "que sais-tu faire ?", "help", "à quoi tu sers ?", "ayuda"]),
    # ---- collaboration, accessibility
    "open_server_browser": ("", ["ouvre les serveurs", "rejoins une session en ligne", "collaboration",
                                  "open the servers"]),
    "copy_invitation_link": ("", ["copie le lien d'invitation", "invite quelqu'un", "copy the invite link"]),
    "disconnect": ("", ["déconnecte-moi", "quitte la session", "disconnect"]),
    "toggle_screen_reader": ("", ["active la lecture vocale", "lecteur d'écran", "screen reader",
                                   "désactive la narration"]),
}

# Sentences no tool serves: the right answer is no call.
NO_CALL = ["raconte-moi une blague", "quel temps fait-il ?", "bonjour, ça va ?", "tu es qui ?", "merci",
           "j'ai faim", "what's the weather?", "écris-moi un poème", "qui a gagné le match ?"]

PREFIXES = ["", "", "", "", "Needle, ", "Stp ", "Vas-y, ", "Ok ", "Allez, ", "Hey, "]
SUFFIXES = ["", "", "", "", " stp", " s'il te plaît", " merci", "!", ".", " please"]


def strip_accents(text):
    return "".join(c for c in unicodedata.normalize("NFD", text) if unicodedata.category(c) != "Mn")


def vary(sentence, rng):
    """Spelling as people really type in a hurry: case, accents, politeness."""
    sentence = rng.choice(PREFIXES) + sentence + rng.choice(SUFFIXES)
    roll = rng.random()
    if roll < 0.15:
        sentence = strip_accents(sentence)
    elif roll < 0.25:
        sentence = sentence.lower()
    elif roll < 0.3:
        sentence = sentence.upper()
    if sentence and rng.random() < 0.6:
        sentence = sentence[0].upper() + sentence[1:]
    return sentence


def facts(rng, mentioned):
    characters = rng.sample(CHARACTERS, rng.randint(2, 7))
    for name in mentioned:
        if name not in characters and rng.random() < 0.85:
            characters.append(name)
    selected = None
    if rng.random() < 0.6:
        selected = [rng.choice(TEXTS), rng.choice(characters), rng.choice([0, 0, 0, 1, 3])]
    fps = rng.choice([25.0, 24.0, 23.976, 30.0])
    minutes = rng.randint(0, 20)
    seconds = rng.randint(0, 59)
    bands = rng.choice([["VF"], ["VF", "EN"], ["Français", "Anglais", "Espagnol"], ["FR"]])
    return {
        "date": "2026-%02d-%02d" % (rng.randint(1, 12), rng.randint(1, 28)),
        "language": rng.choice(["fr", "fr", "fr", "en", "es"]),
        "playhead": "00:%02d:%02d:%02d" % (minutes, seconds, rng.randint(0, 23)),
        "fps": fps,
        "duration": "00:%02d:00:00" % rng.randint(21, 59) if rng.random() < 0.8 else None,
        "selected": selected,
        "characters": characters,
        "bands": bands,
        "shown_band": bands[0],
    }


def expand(tool, spec, template, rng):
    params = {}
    for pair in filter(None, spec.split(";")):
        param, slot = pair.split("=")
        params[slot] = param
    values = {}
    arguments = {}
    mentioned = []
    for slot, param in params.items():
        token = "{" + slot + "}"
        if token not in template:
            continue
        if slot == "BR":
            shown, value = rng.choice(BREATHS)
            values[slot] = shown
            arguments[param] = value
            continue
        value = rng.choice(SLOTS[slot])
        if slot == "C2":
            while value == values.get("C"):
                value = rng.choice(SLOTS[slot])
        values[slot] = value
        if slot in ("C", "C2"):
            mentioned.append(value)
        kind_number = slot in ("SEC", "PCT", "MS", "SPEED", "PX", "OFF", "N", "TR")
        arguments[param] = (float(value) if "." in value else int(value)) if kind_number else value
    sentence = template
    for slot, value in values.items():
        sentence = sentence.replace("{" + slot + "}", value)
    reasoning = "; ".join("'%s' -> %s" % (values[slot], param) for slot, param in params.items() if slot in values)
    return sentence, {"name": tool, "arguments": arguments}, reasoning or ("-> " + tool), mentioned


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("out")
    parser.add_argument("--per-template", type=int, default=4)
    parser.add_argument("--seed", type=int, default=7)
    args = parser.parse_args()
    rng = random.Random(args.seed)
    rows = []
    for tool, (spec, templates) in TOOLS.items():
        for template in templates:
            has_slots = "{" in template
            for _ in range(args.per_template if has_slots else max(1, args.per_template // 2)):
                sentence, call, reasoning, mentioned = expand(tool, spec, template, rng)
                rows.append({"query": vary(sentence, rng), "answers": [call], "reasoning": reasoning,
                             "facts": facts(rng, mentioned)})
    for sentence in NO_CALL:
        for _ in range(2):
            rows.append({"query": vary(sentence, rng), "answers": [], "reasoning": "no tool fits the request",
                         "facts": facts(rng, [])})
    rng.shuffle(rows)
    with open(args.out, "w", encoding="utf-8") as out:
        for row in rows:
            out.write(json.dumps(row, ensure_ascii=False) + "\n")
    print("%d queries for %d tools -> %s" % (len(rows), len(TOOLS), args.out))


if __name__ == "__main__":
    main()
