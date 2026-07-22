# 3.6.0

## Correctifs d’édition, de lecture et de chargement

- Le caret des étiquettes de personnages et d’ambiance suit désormais précisément les glyphes de la police rythmo, leur alignement et leur éventuelle réduction.
- Les espaces saisis dans les noms d’ambiance sont conservés, y compris entre deux mots.
- Les noms d’ambiance sont accolés à leur flèche de début et n’héritent plus de l’espace de quatre images réservé aux étiquettes de personnages.
- Pendant la lecture ou le scrub, le survol et les mouvements de souris sur la bande rythmo respectent la cadence de rafraîchissement de l’application et ne provoquent plus de tempête de rendu.
- En lecture, les mouvements passifs au-dessus de la bande ne déclenchent plus le hit-testing de la timeline ; les gestes actifs restent prioritaires et fluides.
- Le lint global et ses sévérités sont indexés par révision du projet au lieu d’être recalculés et rescannés plusieurs fois par image.
- Après un scrub, le décodeur vidéo et l’audio sont préchargés en arrière-plan : le retour en lecture réutilise ce pipeline chaud sans attente bloquante de FFmpeg.
- Le défilement de la bande utilise désormais une horloge visuelle monotone et continue, indépendante des variations de blocs et de latence du callback audio.
- La présentation GPU reste obligatoirement synchronisée au rafraîchissement de l’écran, même si une ancienne configuration avait désactivé la VSync.
- La bande rythmo possède de nouveau sa cadence dédiée de 240 Hz : sa position fractionnaire est interpolée entre les images vidéo, indépendamment des 24/25/30/60 fps du média et de la cadence des autres animations de l’interface.
- Le texte mobile utilise désormais un pipeline alpha prémultiplié dédié : ses contours ne sont plus multipliés deux fois aux positions sous-pixel, ce qui supprimait leur netteté pendant le défilement.
- Les liaisons d’ambiance visibles sont obtenues via l’index temporel au lieu de rescanner toutes les lignes du projet à chacun des 240 ticks par seconde.
- Le pacer mesure chaque intervalle depuis le début de la frame : le temps de rendu d’un gros projet n’est plus ajouté une seconde fois au délai de rafraîchissement.
- Les médias extraits lors du chargement d’un projet utilisent maintenant le dossier dédié `coquerythmo-temp`, placé dans le répertoire d’installation à côté de l’exécutable.
- À chaque démarrage, Coquerythmo nettoie ce nouveau dossier ainsi que l’ancien `%TEMP%\coquerythmo-projects`, afin de supprimer les fichiers laissés par une fermeture interrompue ou une version précédente.

## Lignes d'ambiance

- Les anciennes liaisons gauche et droite deviennent les actions `Fin d'ambiance` et `Début d'ambiance`, dessinées directement sur la bande rythmo.
- Le début affiche une étiquette `amb.` ineffaçable, bleue, grasse, italique et doublement soulignée dans la police rythmo choisie par l'utilisateur. Elle s'agrandit vers la gauche pour conserver le nom complet, sans couper les graphèmes.
- La fin ne comporte pas d'étiquette : sa description est éditable, s'étend vers la gauche et se termine par une grande liaison blanche pleine. Le début utilise la liaison inverse avant sa description.
- Le texte descriptif est toujours rouge et un espace est réservé aux liaisons afin qu'elles ne soient jamais recouvertes.
- Les noms d'ambiance possèdent leur propre liste d'autocomplétion, distincte de celle des personnages.
- Ces lignes n'acceptent aucun point de synchronisation et ne déclenchent qu'une seule convention de lint : leur contenu doit être placé entre parenthèses.
- Elles sont conservées dans les projets et rendues dans les exports vidéo MP4, mais sont exclues des croisées et des exports documentaires (CSV, sous-titres, DETX, PDF et livrables similaires).
- Le placement du caret tient compte du préfixe protégé, des glyphes de la police choisie et de l'espace réservé à la liaison.

## Étiquettes de personnages

- Les noms de personnages reprennent la typographie des étiquettes d'ambiance : texte gras et italique, coloré avec la couleur du personnage et accompagné d'un double soulignage limité à la largeur réelle des graphèmes.
- L'étiquette s'étend vers la gauche pour afficher le nom complet, occupe toute la hauteur de sa ligne de dialogue et conserve au moins quatre images d'espace avant la réplique.
- Le raster réserve les débords des glyphes italiques afin que le premier et le dernier graphème ne soient plus coupés.
- En lecture karaoké, l'étiquette reste attachée à la ligne de dialogue réellement affichée, y compris dans une piste empilée ; elle utilise la même taille lisible et n'hérite plus de la hauteur de toute la piste.
- Lorsqu'une étiquette rencontre une ligne d'un autre personnage, elle réduit d'abord l'espace avec sa propre réplique. Si cela ne suffit pas, sa largeur, sa hauteur, son texte, ses soulignages et ses icônes diminuent uniformément jusqu'à tenir dans l'espace disponible, tandis que son bord supérieur reste ancré à celui de la ligne.

- Bande rythmo : retrait du glow autour de la barre rouge de précision.

- Detections : `Alt+D` distingue la fleche « bouche ouverte » de la vague d’ouverture ; la palette est reordonnee par famille et son infobulle affiche une bouche agrandie sans recouvrir les boutons.
- Sur une ligne deja soulignee, l’option active est masquee, l’alternative reste disponible et une action permet de retirer le soulignage.
- Detections: `Alt+D` distinguishes the open-mouth arrow from the opening wave; the palette is ordered by sign family and its tooltip shows a larger mouth without covering the buttons.

## Contrôle des conventions de la bande rythmo

- ajout d’un lint non bloquant, visible uniquement dans l’éditeur et absent des projets sérialisés comme des exports ;
- les erreurs certaines sont soulignées par une vague rouge et les avertissements par une vague jaune ; les diagnostics de boucle peuvent couvrir une zone entière en dehors des lignes de dialogue ;
- le survol d’une vague affiche une infobulle compacte, limitée en largeur et répartie sur plusieurs lignes, avec « Avertissement : » en jaune ou « Non conforme : » en rouge ;
- AccessKit lit les diagnostics de la ligne et de la boucle qui la contient à la fin de sa description habituelle ;
- signale en rouge les descriptifs de réaction écrits entre piquants et les boucles de plus d’une minute trente ;
- avertit pour les réactions seules qui devraient utiliser la forme `([Réaction])`, les boucles de plus d’une minute, les phrases sans ponctuation finale, les personnages répartis sur plusieurs pistes et ceux qui mélangent des répliques ON et OFF ;
- les réactions abrégées proposées par l’application, comme `(mhm)` ou `(ah)`, ne déclenchent pas l’avertissement sur les crochets ;
- la durée d’une boucle suit la même délimitation que les exports : du marqueur Boucle au premier OUT ou à la boucle suivante, puis jusqu’à la fin du contenu si la dernière boucle ne possède pas d’OUT.

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
