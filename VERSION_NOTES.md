# 3.6.0

## Lecture audio

- Correction du décalage positif de l'audio instrumental dans l'interface et à l'export : la lecture et les fichiers exportés respectent désormais le silence initial demandé avant de démarrer l'instrumental, y compris lorsque la cadence d'export diffère de celle de la source.

## Export audio et annonceur karaoké

- Ajout de variantes audio sélectionnables séparément pour chaque langue dans le centre d’export :
  - audio original (`O`) ;
  - audio instrumental (`I`) ;
  - audio original avec annonceur (`O+`) ;
  - audio instrumental avec annonceur (`I+`).
- Sous Windows, les variantes avec annonceur utilisent la voix SAPI Windows à 150 %.
- L’annonce est calée pour se terminer une seconde avant le début du chant.
- Les changements sont évalués indépendamment sur chaque piste karaoké ; les lignes normales sont ignorées.
- Les changements simultanés sont regroupés dans une seule annonce, par exemple « Alice, Bob et Chloé ».
- Les variantes sont disponibles pour les exports vidéo MP4 et les exports audio seuls.
- Les contrôles d’export disposent d’un espacement uniforme et suivent le même ordre en affichage et au clavier : `O`, `I`, `O+`, `I+`.

## Réparation du karaoké
- les lignes karaoké n'apparaissent qu'à partir du compte à rebours de leur propre balle ;
- les lignes serrées ne sont plus dessinées les unes sur les autres sur une même rangée ;
- l'onglet Enregistrement et les commandes du mini-DAW sont bloqués lorsque DEV_MODE est désactivé ;
- aucune barre d'onglets vide n'est conservée en production.
- centralise la sélection de la ligne karaoké visible par rangée dans la scène de rendu ;
- donne toujours priorité à la ligne actuellement active sur les aperçus futurs ;
- lorsqu’aucune ligne n’est active, conserve l’aperçu futur le plus proche au lieu du plus lointain ;
- applique la même règle à l’éditeur interactif, au rendu CPU, au rendu GPU et aux exports via la scène partagée ;
- ajoute une régression avec des lignes de 12 frames à 24 FPS, soit 0,5 seconde.


## Syllabification par langue de rythmo - broken
- ajoute un réglage Français / Anglais propre à chaque langue du projet dans le panneau Langues ;
- utilise ce réglage pour le découpage des syllabes, les poignées d’édition, la découpe de dialogue, le karaoké et les exports ;
- transmet la même valeur à la scène de rendu, au rendu CPU et au rendu GPU ;
- invalide les timings syllabiques manuels uniquement pour la langue dont la règle change ;
- ajoute une navigation clavier déterministe, un rôle/une valeur accessibles et des annonces de changement ;


## Détections et points de synchronisation

- les signes de détection sont rattachés à une piste et à un temps média absolu, indépendamment des lignes de dialogue ;
- la palette expose neuf signes professionnels : labiale, semi-labiale, bouche ouverte, bouche fermée, dents visibles, TH, respiration, neutre et réaction ;
- `Alt+D` ouvre la palette tandis que `D` conserve sa navigation horizontale existante ;
- seuls les neuf SVG utilisés sont conservés sous `src/icons/detection` ;
- les signes sont sélectionnables, déplaçables, navigables, ajustables, supprimables et pris en charge par l’historique ;
- un clic simple sur un signe ouvre une grande fiche avec une bouche Rhubarb, son nom, sa - AccessKit lit l’intégralité de cette fiche à son ouverture : nom, description et sons correspondants ;
- la palette `Alt+D`, ses infobulles rapides et la fiche sont composées dans la dernière couche modale afin de rester au-dessus de toute l’interface ;
- le survol d’un choix de la palette affiche immédiatement son nom et ses sons entre parenthèses ;
- le fond opaque du sigle masque le trait vertical dans la bulle tout en conservant exactement le même axe ;
- un glisser démarre directement depuis un signe existant ou depuis un nouveau point de synchronisation, avec un seuil de quatre pixels pour distinguer clic et déplacement ;
- les signes sont ancrés en bas de leur ligne et leur trait vertical reste aligné sur leur axe ;
- `Ctrl+Espace` écoute les deux secondes disponibles avant et après le signe sélectionné avec un bip exactement au repère ;
- les lignes de dialogue affichent des points de synchronisation par caractère, persistants et déplaçables ;
- les opérations d’ajout, déplacement et suppression conservent leurs annonces AccessKit synthétiques ;
nt ou suppression du signe ou du point ;
- les détections et points de synchronisation participent à la persistance, à la conversion de cadence et à la collaboration.

L’étiquette de Nom ne doit pas être collée à la phrase. Afin de ne pas faire perdre de temps à l’adaptateur
(l’adaptation peut débuter avant la flèche de début ou avant le premier mot du texte VO) et pour améliorer la
lisibilité sur le plateau, il faut placer l’étiquette au moins 4 images avant le premier signe ou le texte VO


source detections are attached to a track and an absolute media time, independently of dialogue lines
detections can be placed on any rythmo track even when no dialogue exists
the palette exposes nine professional signs: labial, semi-labial, mouth open, mouth closed, teeth visible, TH, breath, neutral and reaction
Alt+D opens the palette at the current mouse cursor rather than under the hovered track
the visible + is now rendered and hit-tested entirely inside the hovered track, so approaching it no longer switches to the adjacent line and makes it disappear
clicking the visible + opens the same foreground palette atomically instead of depending on the legacy popup hand-off
the palette is a true input-capturing popup: Left/Up and Right/Down move through choices with wrapping, Home/End jump to the edges, Enter activates the selected sign, and Escape closes it
keyboard selection is announced with the same quick label shown visually: Sign name (corresponding sounds)
source detection signs can be dragged vertically onto another track; starting the drag with Shift held preserves the original x/time while changing tracks
cross-track relocation removes the old source cue and creates/selects the corresponding cue in the target track
the palette, quick tooltip and information card remain in the final modal-overlay pass above ordinary panels, menus, labels, icons and toasts
the legacy palette/card render tail is removed before composition, so only one popup is visible
clicking inside the information card no longer creates or shifts a second card; its initial position is preserved until dismissal
the Rhubarb mouth is resized once to the actual 136 px card area with Lanczos filtering, then rendered on an integer-aligned pixel grid; the sub-pixel rows that darkened the image are gone
src/icons/detection retains only the nine SVGs used by the palette; rhubarb_lips/ remains unchanged
source signs are anchored at the bottom of their track while the vertical guide stays centered on the sign axis
an opaque badge masks the vertical guide inside the sign bubble without moving either axis or replacing the original SVG
pressing an existing detection or synchronization point selects it and immediately arms dragging
pressing a synchronization placeholder creates, selects and arms the new point in the same interaction
a 4 px movement threshold distinguishes a click from a drag
synchronization points remain chronologically ordered between their previous and next neighboring points, but may move outside the current line bounds
syllable-handle redistribution is restricted to the interval bounded by the previous and next synchronization points; a handle carrying an exact synchronization anchor stays fixed
moving synchronized text beyond either edge expands the line bounds; leaving unused space at the start or end shrinks them again while retaining every absolute synchronization point
opening a detection information card emits one AccessKit Opened event containing the complete title, description and sound list
signs can be selected, dragged, navigated, nudged, deleted, undone and redone
Ctrl+Space auditions the available two seconds before and after the selected sign and mixes a short beep at the exact cue position
dialogue lines expose per-character synchronization dots whose timing can be dragged directly
detection and synchronization data participate in project persistence, FPS conversion and collaboration synchronization