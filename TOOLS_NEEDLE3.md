# Tools de Needle 3

Needle 3 est l'assistant local de Coquerythmo. `Ctrl + Entrée` dans la Bande
rythmo ouvre la bulle « Que puis-je faire pour vous ? » ; la demande est
envoyée au moteur (`needle.exe` + `needle3.cact`, dossier `needle3` à côté de
l'application, ou `src/ai/needle3` en développement) qui répond par des
appels de tools, exécutés par l'app comme si l'utilisateur avait fait l'action
à la main (annulables avec `Ctrl + Z`, synchronisés en collaboration, annoncés
à l'accessibilité).

Ce fichier est la liste de référence de **tous** les tools. Il doit être mis
à jour dans le même changement que `src/ai/tools.rs` (déclaration, envoyée au
moteur) et `src/ai/dispatch.rs` (exécution) ; voir `AGENTS.md`.

Périmètre actuel : l'onglet **Bande rythmo** uniquement, y compris ce qu'on y
ouvre (export, proxy, paramètres, style de la bande, fichiers du projet). Les
onglets Comic Dubs, Enregistrement et Voicelines viendront plus tard.

## Comment ça marche

Le moteur est un petit modèle (121 M de paramètres, contexte de 8192 tokens)
faible en français : il choisit mal entre plusieurs tools et invente les
arguments qu'on ne lui a pas dictés. L'app compense :

- **Les mots choisissent le tool.** Chaque tool déclare des mots-déclencheurs
  (`triggers`, radicaux sans accent en français, anglais et espagnol), verbes
  et objets (« ajoute » + « boucle »), pondérés par leur rareté dans le
  catalogue (`src/ai/retrieval.rs`). Le moteur ne reçoit que le tool en tête
  quand il domine nettement, sinon les 2 ou 3 meilleurs. Un tool que personne
  ne peut atteindre par ses déclencheurs ne sera jamais appelé.
- **Un tool par variante plutôt qu'un paramètre.** `add_loop`,
  `add_out_point`, `add_scene_change` plutôt que `add_marker{kind}` :
  le choix est fait par les mots, pas par le modèle.
- **Peu d'arguments, et seulement ceux que la phrase dit.** Un argument
  optionnel non dicté est inventé par le modèle ; le moteur le signale
  (« ungrounded ») et l'app le retire. Un appel que le moteur retient par
  manque de confiance est quand même exécuté quand les mots ne laissent pas
  de doute sur le tool ; si le moteur ne répond rien, l'app appelle ce tool
  elle-même.
- **L'app relit la phrase** (`src/ai/query.rs`) plutôt que de croire le
  modèle : une position est prise telle qu'écrite (« 1 min 30 », le modèle
  la convertissait parfois en « 130 seconds ») ; « piste 2 » ou « ligne 2 »
  donne la piste ; texte entre guillemets ou après « : » pour `text`/`note` ;
  personnage connu cité dans la phrase pour `character` ; « X en Y » ou deux
  noms entre guillemets pour `rename_character` (« le personnage de la
  ligne » change le personnage de la ligne sélectionnée, comme en tapant son
  nom dans le champ personnage) ; alias de langue (« anglais » → bande
  « EN ») pour les bandes.
- **On peut désigner une ligne existante.** Les tools qui agissent sur la
  sélection agissent sur la ligne que la phrase désigne, s'il y en a une :
  « celle qui dit "…" », « la ligne qui contient … », « la toute première
  ligne », « la dernière réplique de Marie », « l'avant-dernière réplique »,
  « la ligne ici ». Elle est sélectionnée avant l'action.
- **Une phrase = une action.** Une demande en plusieurs phrases (« Crée une
  réplique piste 2 qui dit Hello. Positionne-la au niveau de la première
  ligne de Chanson ») est découpée (points, « puis », « ensuite », jamais à
  l'intérieur de guillemets) et chaque phrase est traitée à la suite, avec
  l'état laissé par la précédente (la ligne créée est sélectionnée, donc
  « positionne-la » agit dessus). Une phrase qui précise la précédente
  (« Celle qui dit "…" ») lui reste attachée.
- **Modèle fine-tuné.** Le modèle livré peut être affiné sur le catalogue
  (`tools/needle3_finetune/`, voir son README) : les données sont produites
  par le code de l'app (mêmes tools sélectionnés, même texte de session).
  Après tout ajout ou changement de tool, régénérer les données et
  réentraîner.
- **Faits de session.** Le moteur reçoit la date, la langue, le timecode
  courant, la ligne sélectionnée, la liste des personnages et des bandes.
- **Réponse.** Un tool d'édition affiche « ✓ Nom du tool » ; un tool
  d'information affiche sa réponse dans la bulle ; une demande incomprise
  affiche la suggestion du moteur. Fermer la bulle pendant que le moteur
  réfléchit abandonne la demande.
- **Garde-fous.** Les actions qui perdent des données (fermer, nouveau
  projet, supprimer une bande) passent par les mêmes confirmations que
  l'action manuelle. Pendant un export ou un proxy, les actions bloquées le
  restent (toast habituel). Une phrase négative (« ne sauvegarde pas »)
  n'exécute rien.

Diagnostic d'une phrase (tools choisis + réponse brute du moteur) :
`NEEDLE_QUERY="va à 1:30||ajoute une boucle" cargo test --lib ai::needle::tests::engine_query -- --ignored --nocapture`.

Statuts : **OK** = implémenté ; **prévu** = listé pour la version suivante,
pas encore dans le catalogue.

## Lecture

| Tool | Paramètres | Effet | Équivalent manuel | Statut |
|---|---|---|---|---|
| `play` | — | Lance la lecture (ne fait rien si elle est déjà en cours) | `Espace` | OK |
| `pause` | — | Met en pause | `Espace` | OK |
| `play_reverse` | — | Lecture inversée | `Maj + Espace` | OK |
| `seek_to_time` | `time` | Place la tête de lecture à un timecode (`1:30`, `1 minute 30`, `00:01:30:12`, `90s`, `image 120`) | Clic sur la timeline | OK |
| `seek_relative` | `seconds` (négatif = recule) | Avance ou recule de la durée dite | `Q` / `D` | OK |
| `next_frame` | — | Image suivante | `Ctrl + →` | OK |
| `previous_frame` | — | Image précédente | `Ctrl + ←` | OK |
| `step_frames` | `count` (négatif = recule) | Avance de plusieurs images | `Ctrl + ←` / `→` | OK |
| `go_to_start` | — | Retour au début | — | OK |
| `go_to_end` | — | Aller à la fin | — | OK |
| `go_to_next_loop` | — | Boucle suivante | `Ctrl + Maj + molette` | OK |
| `go_to_previous_loop` | — | Boucle précédente | `Ctrl + Maj + molette` | OK |
| `set_volume` | `percent` 0–100 | Règle le volume à la valeur dite | `Maj + ↑` / `↓` | OK |
| `volume_up` | — | Monte le son de 10 % | `Maj + ↑` | OK |
| `volume_down` | — | Baisse le son de 10 % | `Maj + ↓` | OK |
| `mute` | — | Coupe le son (si ce n'est pas déjà fait) | `Maj + Numpad -` | OK |
| `unmute` | — | Remet le son | `Maj + Numpad -` | OK |
| `switch_audio` | — | Bascule audio original / instrumental | `Ctrl + Tab` | OK |
| `offset_audio` | `milliseconds` (négatif = plus tôt) | Décale la piste audio active | Mode décalage audio | OK |
| `open_secondary_display` | — | Ouvre la fenêtre secondaire | Menu Affichage | OK |
| `close_secondary_display` | — | Ferme la fenêtre secondaire | `Échap` dans la fenêtre | OK |
| `set_playback_speed` | `rate` | Vitesse de lecture | — (pas de vitesse dans l'app) | prévu |

## Sélection

| Tool | Paramètres | Effet | Équivalent manuel | Statut |
|---|---|---|---|---|
| `select_line_at_playhead` | — | Sélectionne la ligne sous la tête de lecture (cycle) | `Entrée` | OK |
| `select_next_line` | — | Ligne suivante (déplace aussi la lecture) | `Maj + →` | OK |
| `select_previous_line` | — | Ligne précédente | `Maj + ←` | OK |
| `select_all` | — | Tout sélectionner | `Ctrl + A` | OK |
| `select_lines` | `character`, `text` (au moins un) | Sélectionne les lignes d'un personnage ou contenant un texte | `Maj + clic-glissé` | OK |
| `clear_selection` | — | Désélectionne | `Échap` | OK |

## Répliques

| Tool | Paramètres | Effet | Équivalent manuel | Statut |
|---|---|---|---|---|
| `create_line` | `text`, `character`, `track` 1–4 | Crée une réplique à la tête de lecture avec son texte (cité ou après « : ») et son personnage, puis la sélectionne. Personnage inconnu : créé avec une couleur stable | `Ctrl + Numpad 1–4`, clic maintenu | OK |
| `set_line_text` | `text` | Remplace le texte de la ligne sélectionnée (recale emotions, styles et points de synchro) | `T` | OK |
| `edit_line_text` | — | Ouvre l'éditeur de texte de la ligne sélectionnée | `T` | OK |
| `edit_line_sections` | — | Édition section par section | `Ctrl + T` | OK |
| `set_line_character` | `character` | Attribue un personnage aux lignes sélectionnées | `P` | OK |
| `edit_line_character` | — | Ouvre le champ personnage | `P` | OK |
| `delete_selected_lines` | — | Supprime les lignes sélectionnées | `Suppr` | OK |
| `delete_lines` | `character`, `text` (au moins un) | Supprime les lignes d'un personnage / contenant un texte | Tableau des lignes | OK |
| `copy_lines` | — | Copie la sélection | `Ctrl + C` | OK |
| `cut_lines` | — | Coupe la sélection | `Ctrl + X` | OK |
| `paste_lines` | — | Colle à la tête de lecture | `Ctrl + V` | OK |
| `paste_lines_with_track_character` | — | Colle avec le personnage de la piste | `Ctrl + Maj + V` | OK |
| `move_lines_to_track` | `track` 1–4 | Déplace la sélection sur une piste | `↑` / `↓` | OK |
| `move_lines_to_time` | `time` | Place la sélection à un timecode (les écarts entre lignes sont gardés) | Glisser la ligne | OK |
| `move_lines_to_character_line` | `character`, `line_index` (1 = première, -1 = dernière) | Place la sélection au début d'une réplique d'un autre personnage | Glisser la ligne | OK |
| `shift_lines` | `seconds` ou `frames` (négatif = gauche) | Décale la sélection sans changer la durée | `Ctrl + Maj + ←` / `→` | OK |
| `set_line_start` | — | Fixe le début de la ligne sélectionnée à la tête de lecture | `I` | OK |
| `set_line_end` | — | Fixe la fin de la ligne sélectionnée à la tête de lecture | `O` | OK |
| `set_line_duration` | `seconds` | Règle la durée de la ligne sélectionnée | Glisser le bord | OK |
| `split_line` | — | Scinde au caret ou à la tête de lecture | `Ctrl + K` | OK |
| `toggle_karaoke` | — | Karaoké on/off sur la sélection | `Numpad 9` | OK |
| `toggle_bold` | — | Gras | `Ctrl + B` | OK |
| `toggle_italic` | — | Italique | `Ctrl + I` pendant l'édition | OK |
| `toggle_underline` | — | Souligné | `Ctrl + U` | OK |
| `toggle_strikethrough` | — | Barré | Barre de mise en forme | OK |
| `set_text_emotion` | `emotion` (famille + intensité : « colère extrême », « un peu de joie », « question rapide », « shake ») | Applique une émotion à toute la ligne sélectionnée | `Alt + E` | OK |
| `clear_text_emotion` | — | Retire l'émotion de la ligne sélectionnée | `Alt + E` | OK |
| `open_emotion_menu` | — | Ouvre le menu des émotions | `Alt + E` | OK |
| `link_lines` | — | Lie les dialogues sélectionnés (≥ 2) | Clic droit > Dialogues liés | OK |
| `link_lines_with_character` | — | Lie les dialogues, personnage inclus | Clic droit > Dialogues liés | OK |
| `unlink_lines` | — | Délie le groupe de la ligne sélectionnée | Clic droit > Délier | OK |
| `set_line_note` | `note` | Écrit la note de la ligne sélectionnée | `Numpad 6` | OK |
| `add_note` | — | Ouvre la note de la ligne sélectionnée | `Numpad 6` | OK |
| `set_line_on_screen` | — | Dans le champ | Palette de détection | OK |
| `set_line_off_screen` | — | Hors champ | Palette de détection | OK |
| `set_line_back` | — | De dos | Palette de détection | OK |
| `generate_detection_signs` | — | Convertit la ligne en signes de détection | Clic droit | OK |
| `undo` | — | Annule | `Ctrl + Z` | OK |
| `redo` | — | Rétablit | `Ctrl + Y` | OK |
| `set_text_emotion_range` | `emotion`, `from`, `to` | Émotion sur une partie du texte | Sélection de texte + `Alt + E` | prévu |
| `set_syllable_timing` | `syllable`, `time` | Calage d'une syllabe karaoké | Glisser la syllabe | prévu |
| `set_character_karaoke_dot` | `character`, `dot` | Point karaoké d'un personnage | Style de la bande | prévu |

## Marqueurs et signes

| Tool | Paramètres | Effet | Équivalent manuel | Statut |
|---|---|---|---|---|
| `add_loop` | — | Boucle à la tête de lecture | `Numpad 1` | OK |
| `add_out_point` | — | Point Out à la tête de lecture | `Numpad 2` | OK |
| `add_scene_change` | — | Changement de plan à la tête de lecture | `Numpad 3` | OK |
| `add_liaison_left` | — | Liaison à gauche (ligne d'ambiance) | `Numpad 7` | OK |
| `add_liaison_right` | — | Liaison à droite | `Numpad 8` | OK |
| `add_breath` | `kind` : `h`, `hh`, `mh`, `mhh`, `up`, `down` (optionnel) | Ajoute une respiration (ligne rapide) | `Numpad 4` | OK |
| `add_reaction` | `kind` : `x`, `mts`, `tsc`, `ah`, `oh`, `ih`, `mhm`, `hm`, `ptt`, `pff`, `unh`, `hun`, `psst` (optionnel) | Ajoute une réaction (ligne rapide) | `Numpad 5` | OK |
| `add_sync_point` | — | Point de synchro à la tête de lecture sur la ligne sélectionnée | `Ctrl + Maj + Espace` | OK |
| `list_markers` | — | Compte les boucles, points Out, changements de plan ; liste les boucles | — | OK |
| `move_marker` | `index`, `time` | Déplace un marqueur | Glisser | prévu |
| `delete_markers` | `kind`, `time` | Supprime des marqueurs | Sélection + `Suppr` | prévu |
| `add_detection_sign` | `kind`, `time`, `track` | Pose un signe de détection (labiale, bouche ouverte…) | `Alt + D` | prévu |
| `delete_detection_sign` | `time`, `track` | Supprime un signe | `Suppr` | prévu |

## Personnages et comédiens

| Tool | Paramètres | Effet | Équivalent manuel | Statut |
|---|---|---|---|---|
| `rename_character` | `old_name`, `new_name` | Renomme partout (« renomme Jean en Pierre ») | Menu Outils > Renommer | OK |
| `set_character_color` | `character`, `color` | Couleur du personnage partout (nom de couleur ou `#rrggbb`) | Panneau des rôles | OK |
| `list_characters` | — | Liste les personnages | `Ctrl + P` | OK |
| `open_roles_panel` | — | Ouvre le panneau des rôles | `Ctrl + P` | OK |
| `open_lines_panel` | — | Ouvre le tableau des lignes | `Ctrl + I` | OK |
| `open_rename_character` | — | Ouvre la fenêtre Renommer | Menu Outils | OK |
| `open_voice_actors` | — | Ouvre les comédiens | Menu Outils | OK |
| `create_voice_actor` | `name` | Crée un comédien (sans icône) | Fenêtre Comédiens | OK |
| `assign_voice_actor_to_line` | `actor` | Attribue un comédien à la ligne sélectionnée | Clic droit | OK |
| `assign_voice_actor_to_character` | `actor` | Attribue un comédien à tout le personnage de la ligne sélectionnée | Clic droit | OK |
| `delete_character` | `character` | Supprime un personnage (réattribue ou supprime ses lignes) | — | prévu |
| `unassign_voice_actor` | `actor` | Retire un comédien | Clic droit | prévu |

## Outils de dessin

| Tool | Paramètres | Effet | Équivalent manuel | Statut |
|---|---|---|---|---|
| `use_select_tool` | — | Outil Sélection | `C` | OK |
| `use_draw_tool` | — | Outil Dessin | `Ctrl + D` | OK |
| `toggle_eraser` | — | Gomme | Barre de dessin | OK |
| `change_brush_size` | — | Taille du pinceau suivante | Barre de dessin | OK |
| `open_brush_color` | — | Couleur du pinceau | Barre de dessin | OK |
| `clear_drawings` | — | Efface tous les traits | — | prévu |

## Export, fenêtres et panneaux

| Tool | Paramètres | Effet | Équivalent manuel | Statut |
|---|---|---|---|---|
| `open_export` | — | Ouvre l'export (vidéo, sous-titres, audio, références) | `Ctrl + M` | OK |
| `export_subtitles` | — | Ouvre l'export et rappelle les formats texte | `Ctrl + M` | OK |
| `cancel_export` | — | Annule l'export ou le proxy en cours | `Échap` | OK |
| `open_proxy` | — | Ouvre la création de proxy | `Ctrl + Maj + P` | OK |
| `open_project_settings` | — | Paramètres du projet | `Ctrl + O` | OK |
| `set_scroll_speed` | `value` 0,25–4 | Vitesse de défilement | `Ctrl + O` | OK |
| `set_reading_bar_offset` | `value` -50–50 (%) | Position de la barre de lecture | `Ctrl + O` | OK |
| `toggle_highlight_read_word` | — | Surligner le mot lu on/off | `Ctrl + O` | OK |
| `toggle_character_color_text` | — | Texte à la couleur du personnage on/off | `Ctrl + O` | OK |
| `toggle_emotion_lanes` | — | Pistes d'émotions on/off | `Ctrl + O` | OK |
| `toggle_timecode_display` | — | Timecode dans la barre d'outils on/off | `Ctrl + O` | OK |
| `set_start_timecode` | `value` | Timecode de la première image | `Ctrl + O` | OK |
| `set_waveform_height` | `value` (px) | Hauteur de la forme d'onde | `Ctrl + O` | OK |
| `open_band_style` | — | Style de la bande | `Ctrl + Maj + O` | OK |
| `open_settings` | — | Paramètres de l'application | Menu | OK |
| `show_shortcuts` | — | Affiche le panneau des raccourcis | Menu Paramètres | OK |
| `hide_shortcuts` | — | Masque le panneau des raccourcis | Menu Paramètres | OK |
| `open_automation` | — | Graphe d'automatisation | Menu Outils | OK |
| `toggle_project_files` | — | Fichiers du projet | `Ctrl + L` | OK |
| `open_recent_projects` | — | Projets récents | `Ctrl + R` | OK |
| `close_panel` | — | Ferme panneau latéral, fichiers et automatisation | `Échap` | OK |
| `start_export` | `output`, `formats`, … | Export sans fenêtre, avec les derniers réglages | — | prévu |
| `apply_band_style_preset` | `name` | Applique un preset de style | Style de la bande | prévu |
| `create_proxy` | `width`, `height`, `quality` | Proxy sans fenêtre | `Ctrl + Maj + P` | prévu |

## Projet et bandes

| Tool | Paramètres | Effet | Équivalent manuel | Statut |
|---|---|---|---|---|
| `save_project` | — | Sauvegarde (ou ouvre « Enregistrer sous » la première fois) | `Ctrl + S` | OK |
| `save_project_as` | — | Enregistrer sous | Menu Projet | OK |
| `new_project` | — | Nouveau projet (confirmation si non sauvegardé) | `Ctrl + N` | OK |
| `open_project` | — | Ouvre un projet (sélecteur de fichier) | `Ctrl + Maj + I` | OK |
| `close_project` | — | Ferme le projet (confirmation) | `Ctrl + Suppr` | OK |
| `import_subtitles` | — | Importe un sous-titrage (sélecteur) | `Ctrl + Maj + I` | OK |
| `restore_backup` | — | Restaure la dernière sauvegarde automatique | `Ctrl + Maj + R` | OK |
| `add_video` | — | Ajoute une vidéo (sélecteur) | Menu Projet | OK |
| `add_audio` | — | Ajoute un audio (sélecteur) | Menu Projet | OK |
| `set_instrumental_audio` | — | Choisit l'audio instrumental (sélecteur) | `Ctrl + O` | OK |
| `add_band` | `name` (optionnel) | Crée une bande (nom par défaut si vide) | Fichiers du projet | OK |
| `select_band` | `name` (nom, préfixe ou langue : « anglais » → « EN ») | Affiche une autre bande | Fichiers du projet | OK |
| `rename_band` | `name` | Renomme la bande affichée | `F2` | OK |
| `delete_band` | `name` | Supprime une bande | `Suppr` | OK |
| `list_bands` | — | Liste les bandes et celle affichée | `Ctrl + L` | OK |
| `open_recent_project` | `name` | Ouvre un projet récent par son nom | `Ctrl + R` | prévu |
| `set_band_language` | `name`, `language` | Langue de syllabation d'une bande | Fichiers du projet | prévu |

## Conformation (nouvelle version du montage)

Quand la production envoie une V2 / V3, « Ajouter et conformer » (`Ctrl + L`,
clic droit sur Vidéos ou sur une vidéo) compare l'ancienne et la nouvelle
vidéo, déplace tout ce qui est calé sur le temps (répliques et leurs points de
synchro, détections, boucles et marqueurs, dessins, prises de l'onglet
Enregistrement) et affiche ce qui a été déplacé, modifié, tronqué, supprimé ou
doit être réenregistré. La bande choisie n'est pas modifiée : le résultat va
dans une nouvelle bande « bande – vidéo », où la conformation est une seule
commande annulable (`Ctrl + Z`) ; les prises suivent les opérations
d'enregistrement, qui ne s'annulent pas.

| Tool | Paramètres | Effet | Équivalent manuel | Statut |
|---|---|---|---|---|
| `open_conform` | `video` (nom, opt.), `band` (nom, opt.) | Ouvre « Ajouter et conformer », présélectionne la vidéo et la bande | `Ctrl + L` > clic droit sur Vidéos > Ajouter et conformer | OK |
| `conform_video` | `path` (chemin du fichier), `video` (opt.), `band` (opt.) | Compare et conforme sans passer par le sélecteur de fichier ; ajoute la nouvelle vidéo, l'affiche, ouvre le rapport | Fenêtre « Ajouter et conformer » > 3. Choisir la nouvelle vidéo | OK |
| `get_conform_report` | — | Donne le rapport de la dernière conformation (déplacé, modifié, tronqué, supprimé, à réenregistrer) | Fenêtre « Ajouter et conformer » (rapport) | OK |

## Informations

| Tool | Paramètres | Effet | Équivalent manuel | Statut |
|---|---|---|---|---|
| `get_project_info` | — | Nom, vidéo, durée, cadence, nombre de répliques, bandes, personnages | — | OK |
| `count_lines` | `character` (optionnel) | Nombre de répliques (d'un personnage) | — | OK |
| `find_line` | `text` | Trouve la première ligne contenant le texte, la sélectionne et y va | `Ctrl + I` | OK |
| `get_playhead_time` | — | Timecode et image courants | Barre d'outils | OK |
| `get_selected_line` | — | Texte, personnage et timing de la ligne sélectionnée | — | OK |
| `get_shortcut` | `feature` | Raccourci ou emplacement d'une fonctionnalité (depuis ce catalogue) | `RACCOURCIS_CLAVIER.md` | OK |
| `help` | — | Ce que Needle 3 sait faire | — | OK |
| `get_line_at_time` | `time` | Quelle réplique à ce moment | — | prévu |
| `get_character_lines` | `character` | Liste les répliques d'un personnage | Tableau des lignes | prévu |

## Collaboration

| Tool | Paramètres | Effet | Équivalent manuel | Statut |
|---|---|---|---|---|
| `open_server_browser` | — | Liste des serveurs | Menu Collaboration | OK |
| `copy_invitation_link` | — | Copie le lien d'invitation de la session | Menu Collaboration | OK |
| `disconnect` | — | Quitte la session en ligne | Menu Collaboration | OK |
| `connect` | `server`, `username` | Rejoint un serveur | Menu Collaboration | prévu |

## Accessibilité

| Tool | Paramètres | Effet | Équivalent manuel | Statut |
|---|---|---|---|---|
| `toggle_screen_reader` | — | Lecture vocale interne | `Ctrl + Maj + N` | OK |

## Enregistrement (prévu)

L'onglet Enregistrement n'est pas encore exposé à Needle 3 (la bulle ne
s'ouvre que dans la Bande rythmo). Ses tools sont listés ici pour la version
qui l'ouvrira ; chacun passera par la même `UiAction` que le clavier et la
souris. Les noms de piste, de prise (« la prise d'Alice à 1:30 ») et d'audio
seront reçus en texte et interprétés par l'app.

| Tool | Paramètres | Effet | Équivalent manuel | Statut |
|---|---|---|---|---|
| `choose_recording_mode` | `mode` (solo, en ligne) | Entre dans l'enregistrement solo ou en ligne | Cartes « S'enregistrer en solo / en ligne » | prévu |
| `set_recording_tool` | `tool` (sélection, couper) | Change l'outil de la timeline | Boutons Sélection / Couper | prévu |
| `add_recording_track` | — | Ajoute une piste | Bouton `+` | prévu |
| `delete_recording_track` | `track` | Supprime une piste (pas la dernière) | Bouton `×` de la piste | prévu |
| `rename_recording_track` | `track`, `name` | Renomme une piste | `Entrée` sur le nom de la piste | prévu |
| `set_recording_track_mute` | `track`, `muted` | Coupe ou rétablit le son d'une piste | Bouton M | prévu |
| `set_recording_track_solo` | `track`, `solo` | Met une piste en solo | Bouton S | prévu |
| `arm_recording_track` | `track` | Arme la piste qui reçoit les prises | Bouton R | prévu |
| `set_recording_track_volume` | `track`, `percent` | Volume d'une piste | `←` / `→` sur le volume | prévu |
| `set_recording_track_reverb` | `track`, `enabled` | Réverbe de chant dans l'aperçu | Bouton Rev | prévu |
| `export_recording_track` | `track` | Exporte l'audio d'une piste en FLAC | Bouton E | prévu |
| `start_recording` | — | Lance le compte à rebours puis la prise sur la piste armée | Bouton ● REC | prévu |
| `stop_recording` | — | Termine la prise en cours | `Échap` | prévu |
| `select_take` | `track`, `time` | Sélectionne une prise (sans `time` : celle au curseur de lecture) | `Entrée` sur la prise, ou `Entrée` sans focus | prévu |
| `seek_to_take` | `track`, `time`, `end` | Tête de lecture au début ou à la fin d'une prise | `Début` / `Fin` sur la prise | prévu |
| `move_takes` | `frames` | Décale les prises sélectionnées dans le temps | `Ctrl + Maj + ←` / `→` | prévu |
| `move_takes_to_track` | `direction` (haut, bas) | Passe les prises sélectionnées sur la piste voisine | `Ctrl + Maj + ↑` / `↓` | prévu |
| `cut_take_at_playhead` | — | Coupe la prise sélectionnée au curseur | `Ctrl + K` | prévu |
| `delete_takes` | — | Supprime les prises sélectionnées | `Suppr` | prévu |
| `import_recording_audio` | `username` | Importe un audio dans les audios du projet | Menu Audio › Importer | prévu |
| `place_audio_at_playhead` | `audio` | Place un audio du projet au curseur, sur la piste armée | `Ctrl + V`, menu de l'audio | prévu |
| `delete_recording_audio` | `audio` | Supprime un audio et ses prises | `Suppr`, menu de l'audio | prévu |
| `send_audio_to_voicelines` | `audio` | Envoie un audio vers Voicelines | Menu de l'audio › Envoyer vers | prévu |
| `open_microphone_settings` | — | Choix du micro d'enregistrement | Menu Audio, menu du comédien | prévu |
| `get_recording_tracks` | — | Liste les pistes et leur état (armée, muette, volume, prises) | `Tab` sur le nom des pistes | prévu |
| `get_takes` | `track` | Liste les prises d'une piste (qui, début, durée) | `↑` / `↓` / `←` / `→` sur les prises | prévu |
| `get_participants` | — | Liste les participants, leur rôle et leur micro | Liste des participants | prévu |

## Hors périmètre (autres onglets, plus tard)

Comic Dubs (bulles, plans, pages, sons, export), Voicelines (régions,
export), Whistle (dictée vocale de la demande et des répliques). Les tools
de l'Enregistrement sont listés plus haut, au statut « prévu ».
