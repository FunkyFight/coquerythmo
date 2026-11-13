# 6.0.0

## Nouvel onglet Traitement : retirer la voix d'un audio

Un nouvel onglet **Traitement**, juste avant Enregistrement, retire la voix
d'un audio pour n'en garder que la musique et les bruitages, isole une voix,
sépare des instruments ou nettoie un enregistrement. Disponible sur Windows
64 bits.

- **Apporter un son.** Glissez un audio ou une vidéo dans l'onglet, cliquez
  sur **Importer un audio** ou sur **Audio de la vidéo**. Depuis les fichiers
  du projet (`Ctrl + L`), clic droit sur une vidéo ou un audio > **Envoyer
  dans Traitement**. Pour une vidéo, c'est sa piste son qui est traitée ; les
  fichiers du projet ne sont jamais modifiés.
- **Écouter et comparer.** L'audio d'origine s'affiche en haut, chaque piste
  produite en dessous avec sa forme d'onde. Un clic sur une piste l'écoute à
  cet endroit : on passe de l'original au résultat sans perdre sa place.
  `Ctrl + molette` zoome.
- **Traitements en chaîne.** Le traitement s'applique à la piste en écoute,
  d'origine ou déjà traitée : on peut retirer la voix, puis séparer les
  instruments de l'instrumental obtenu. Chaque piste produite est décalée
  sous celle dont elle descend, reliée par une barre, et dit d'où elle vient.
  Chacune se retire séparément avec sa croix (l'instrumental sans la voix,
  par exemple), avec les pistes qui en descendent.
- **Garder un résultat.** **+ Projet** ajoute la piste comme nouvel audio
  dans les fichiers du projet (`Ctrl + L`) : le fichier est écrit à côté du
  projet, sans rien remplacer, et peut ensuite servir d'instrumental à une
  bande. **Exporter** l'écrit où vous voulez en WAV, FLAC, MP3, OGG ou M4A,
  **Tout exporter** écrit toutes les pistes d'un coup. Une piste ni ajoutée
  ni exportée est perdue à la fermeture de Coquerythmo.
- **Neuf traitements**, à télécharger selon vos besoins depuis le
  **Catalogue des modèles** : Retirer la voix (Mel-Band RoFormer Inst FV7z de
  Gabox), Isoler la voix, Séparer les instruments en 6 pistes (voix,
  batterie, basse, guitare, piano, autres), Retirer le bruit de fond, Retirer
  la réverbération, Isoler les respirations, Retirer la foule, Séparer voix
  d'homme et de femme, Séparer voix principale et chœurs. Chaque modèle fait
  200 à 900 Mo ; plusieurs peuvent se télécharger en même temps. La colonne
  de gauche liste les modèles installés et en cours de téléchargement.
- **Moteur de traitement.** Sa fenêtre s'ouvre à la première visite de
  l'onglet. Il faut **un PC puissant** : une carte graphique NVIDIA RTX
  (série 4000 conseillée) avec 8 Go de mémoire vidéo. La fenêtre affiche
  votre carte ; sans carte NVIDIA, le traitement fonctionne mais il est très
  lent. Vous y **choisissez le dossier d'installation** (environ 3,5 Go à
  télécharger, une dizaine sur le disque) : tout est rangé dans un
  sous-dossier `coquerythmo-processing`. L'installation continue fenêtre
  fermée, et un téléchargement interrompu reprend où il s'était arrêté.
- **Pendant le traitement**, la vitesse et le temps restant sont affichés et
  l'on peut travailler dans les autres onglets. **Qualité : rapide** (par
  défaut) ou **maximale**, environ deux fois plus longue. Si la mémoire vidéo
  manque, les réglages se réduisent tout seuls au lieu d'échouer.
- **Tout se désinstalle** : un modèle depuis la colonne de gauche ou le
  catalogue, le moteur ou l'ensemble depuis sa fenêtre, après un second clic
  de confirmation. Seuls les fichiers installés par Coquerythmo sont effacés.
  Si le dossier choisi n'est pas celui de Coquerythmo, faites **Tout
  désinstaller** avant de désinstaller Coquerythmo.
- **Raccourcis clavier** propres à l'onglet : `Ctrl + Entrée` lance le
  traitement, `Ctrl + Maj + Entrée` l'annule, `Ctrl + ↑` / `↓` change de
  piste écoutée, `Ctrl + Maj + ↑` / `↓` de modèle, `Ctrl + I` importe,
  `Ctrl + E` exporte, `Ctrl + P` ajoute au projet, `Ctrl + M` ouvre le
  catalogue, `Ctrl + B` bascule la qualité. Ils sont listés dans le panneau
  des raccourcis et dans [`RACCOURCIS_CLAVIER.md`](RACCOURCIS_CLAVIER.md).
- **Accessible aux personnes malvoyantes.** Tout l'onglet se pilote au
  clavier et au lecteur d'écran : `Tab` atteint chaque bouton, modèle, audio
  et piste, même hors de l'écran, `↑` / `↓` passent à l'élément voisin,
  `Suppr` retire celui qui a le focus. Chaque élément est lu avec son état et
  son raccourci, les fenêtres lisent leur texte à l'ouverture, les actions et
  la progression sont annoncées, et un bouton qui ne peut pas agir dit
  pourquoi. Pour la basse vision : textes plus grands, contour de focus épais
  et doublé, bordure claire sur ce qui est sélectionné.

## Karaoké : caler les syllabes au clavier

Sur une ligne karaoké sélectionnée, tout se règle au clavier avec `Alt` :
les poignées rouges en haut, les points et les liaisons en dessous.

- **Parcourir.** `Alt + ←` / `Alt + →` passe d'une poignée à l'autre (au
  premier appui, celle de la syllabe sous le curseur de lecture).
  `Alt + ↓` descend sur les points et les liaisons, que les mêmes flèches
  parcourent, `Alt + ↑` remonte aux poignées. Ce qui est choisi est entouré
  d'un halo clair et lu avec son rang et son état.
- **Basculer.** `Entrée` active ou non le point, change la liaison (faite,
  non faite, non indiquée), ou pose la poignée là où elle est (les réglages
  suivants s'arrêtent à elle, un second appui la libère).
- **Déplacer une poignée.** `Ctrl + Alt + ←` / `→` la déplace d'une image.
- **Caler en écoutant.** `Ctrl + Alt + Espace` place la poignée au curseur de
  lecture et passe à la suivante : on lance la lecture et on appuie au début
  de chaque syllabe.
- `Échap` revient à la ligne.
- Le réglage suit les mêmes règles qu'à la souris : les autres poignées
  suivent en proportion, les poignées posées ne bougent plus.
- Chaque action est annoncée, et les raccourcis apparaissent dans le panneau
  des raccourcis dès qu'une ligne karaoké est sélectionnée.

## Karaoké : des poignées de syllabes sur toute la hauteur

- Les poignées rouges entre les syllabes traversent maintenant toute la
  hauteur de la ligne, au lieu d'un petit trait en haut : la limite de chaque
  syllabe se lit d'un coup d'œil.
- Elles s'attrapent sur toute leur hauteur. Pendant la saisie du texte, seul
  le haut de la ligne les attrape, pour qu'un clic dans le texte place
  toujours le curseur.

## Enregistrement : annuler une coupe, revenir au début d'une prise

- `Ctrl + Z` annule la dernière coupe (`Ctrl + K`) : les deux morceaux
  redeviennent une seule prise. Plusieurs coupes s'annulent l'une après
  l'autre. Une coupe dont un morceau a été déplacé seul, raccourci ou
  supprimé ne s'annule plus.
- `Ctrl + Entrée` ramène le curseur de lecture au début de la prise
  sélectionnée.

## Fichiers du projet : les médias embarqués restent utilisables

- Après la réouverture d'un projet `.coquerythmo`, la vidéo, son proxy et
  les instrumentaux embarqués étaient affichés comme introuvables dans les
  fichiers du projet (`Ctrl + L`) alors que la vidéo se lisait : **Utiliser**
  et **Créer un proxy** répondaient « Le fichier média est introuvable ».
  Ils fonctionnent de nouveau, y compris sur un projet reçu d'un autre
  ordinateur ou dont le fichier d'origine a été déplacé.

# 5.5.0

## Ajouter et conformer : suivre une V2 / V3 du montage

- Quand la production livre une nouvelle version du montage, plus besoin de
  tout recaler à la main : dans les fichiers du projet (`Ctrl + L`), clic droit
  sur **Vidéos** (ou sur une vidéo) > **Ajouter et conformer…**. La fenêtre
  demande dans l'ordre la vidéo du projet, la bande rythmo, puis la nouvelle
  vidéo sur le disque.
- Coquerythmo compare alors les deux versions plan par plan (images et son,
  ce qui résiste au réencodage, à l'étalonnage et au changement de
  résolution), retrouve les plans communs et déplace automatiquement tout ce
  qui est calé sur le temps : répliques avec leurs commentaires (notes) et
  leurs points de synchro, détections, boucles, changements de plan et autres
  marqueurs, dessins, et les prises de l'onglet Enregistrement (case à décocher pour une deuxième
  bande conformée sur la même vidéo). Une vidéo dont la cadence a changé est
  recalée à la nouvelle cadence.
- À la fin, un rapport liste ce qui a été **déplacé**, **modifié** (des images
  ont été insérées ou recoupées sous la réplique), **tronqué** (une partie des
  images a disparu), **supprimé** (plus aucune image : la réplique ou le
  marqueur est retiré de la bande) et ce qui est **à réenregistrer** (en rose :
  les répliques recoupées et les prises dont les images ont changé). La
  nouvelle vidéo est ajoutée au projet, devient la vidéo par défaut et
  s'affiche aussitôt.
- La bande choisie n'est jamais modifiée : le résultat va dans une nouvelle
  bande nommée « bande – vidéo » (par exemple « Français – film_V2 »), copie
  conforme de l'originale avec son instrumental et sa langue de découpe, que
  l'on peut renommer ou supprimer comme toute bande. Dans cette nouvelle
  bande, la conformation est une seule commande : `Ctrl + Z` remet tout en
  place. `Échap` pendant l'analyse l'annule sans rien toucher.
- Accessible au clavier et au lecteur d'écran : `Tab` passe de la liste des
  vidéos à celle des bandes, à la case des prises puis aux boutons, les
  flèches choisissent, `Entrée` valide, `Échap` annule ; chaque étape, la
  progression, le résumé du rapport et les lignes affichées sont annoncés.

## Karaoké : des syllabes qui se prononcent

- Le découpage français suit maintenant les syllabes qu'on prononce :
  « aperçois » se découpe `a|per|çois` (au lieu de `aper|çois`),
  « aujourd'hui » `au|jour|d'hui`, « psychologue » `psy|cho|logue`.
- Une élision reste avec la syllabe qu'elle commence : « d'accord » se
  découpe `d'ac|cord` (au lieu de `d'|ac|cord`), « j'adore » `j'a|dore`,
  « qu'il » en une seule syllabe.
- Une syllabe muette en fin de mot (« monstres », « monde », « table »,
  « magnifique ») reste avec la précédente, comme on la dit. Quand elle doit
  être chantée ou marquée, un **pin** (un petit rond) apparaît sous le mot,
  juste en dessous de la ligne karaoké survolée : un clic sépare toutes les
  syllabes (`mons|tres`, `ma|gni|fi|que`), un second clic les réunit. Le pin
  est plein et jaune quand la séparation est active ; il vaut pour toutes les
  occurrences du mot dans la ligne, s'annule avec `Ctrl + Z`, se synchronise
  en collaboration et suit la ligne quand on la copie, la coupe en deux ou
  l'exporte.
- En lecture, un pin actif reste affiché : un petit point jaune sous la
  syllabe indique au comédien qu'elle doit être prononcée. Il apparaît aussi
  dans les vidéos exportées.
- Les calages déjà faits ne bougent pas : chaque lettre garde sa place dans
  le temps, que la ligne ait été calée avec l'ancien découpage ou qu'on
  active un pin après coup (la poignée d'une syllabe muette calée dans une
  version précédente revient à sa place quand on active son pin).
- En anglais et en espagnol, le découpage ne change pas ; le pin y propose
  les syllabes manquantes (« table » `ta|ble`, « being » `be|ing`).

## Karaoké : marquer les liaisons

- Là où une liaison est possible entre deux mots d'une ligne karaoké (« les
  amis », « rien à rajouter », « petit homme », « chez elle », « vas-y »),
  une flèche apparaît sous la ligne survolée, de la consonne finale à la
  voyelle du mot suivant. Un clic la fait passer par trois états : rien
  (flèche pâle, visible seulement au survol), **liaison** (flèche jaune),
  **pas de liaison** (la même flèche barrée d'un trait oblique), puis de
  nouveau rien.
- Toutes les consonnes de liaison sont prises en compte : s, x, z, t (sauf
  « et »), d, n, le r des mots en « -er », le p de « trop » et « beaucoup »,
  le g de « long » et le f de « neuf », devant une voyelle ou un h. Rien
  n'est proposé à travers une virgule ou un point.
- En lecture et dans les vidéos exportées, seules les liaisons marquées sont
  affichées, sous les mots : le comédien voit d'un coup d'œil celles à faire
  et celles à ne pas faire.
- Le marquage vaut pour toutes les fois où les deux mots se suivent dans la
  ligne, s'annule avec `Ctrl + Z`, se synchronise en collaboration et suit la
  ligne quand on la copie, la coupe en deux ou l'exporte.

## Karaoké : le point retombe sur les poignées

- Le point du karaoké retombe maintenant au bon endroit : exactement entre
  deux syllabes du texte affiché, au moment où la lecture atteint la poignée
  rouge correspondante. Il était placé d'après le nombre de lettres de
  chaque syllabe et non d'après leur largeur réelle, ce qui le faisait
  retomber à côté dès qu'une syllabe avait des lettres étroites ou larges
  (« il », « mmm »). Le point est aussi centré sur la frontière au lieu
  d'être décalé d'un demi-point.
- Sur une ligne dont les poignées n'avaient jamais été déplacées (une ligne
  ajoutée après coup pour un autre personnage, par exemple), le point suit
  désormais les poignées affichées au lieu d'une autre répartition.
- La couleur qui remplit le texte suit le même calage, dans l'application
  comme dans les vidéos exportées.
- La syllabe sur laquelle le point arrive se soulève très légèrement et
  ses lettres brillent un instant, puis elle se repose : on voit d'un coup
  d'œil où l'on en
  est dans la ligne. L'effet est le même dans l'application et dans les
  vidéos exportées.
- Un nom de personnage trop long ne sort plus de l'écran en mode karaoké :
  il est réduit pour tenir entre le bord gauche de la bande et sa ligne,
  dans l'application comme à l'export.

## Karaoké : caler les syllabes dans un sens

- Un nouveau bouton, à droite de celui du karaoké dans la barre d'outils,
  choisit le sens du calage proportionnel : **de gauche à droite** (par
  défaut) ou **de droite à gauche**. Le choix est conservé d'une session à
  l'autre.
- Le premier déplacement d'une poignée répartit toute la ligne, comme avant.
  Ensuite, de gauche à droite, tout ce qui est à gauche de la poignée qu'on
  vient de placer (elle comprise) ne bouge plus : déplacer une poignée plus
  loin ne répartit que ce qui reste à sa droite. On cale ainsi la ligne par
  moitiés successives, sans défaire ce qui est déjà en place. De droite à
  gauche, c'est le côté droit qui reste en place.
- Une poignée placée est dessinée en jaune. Revenir sur une poignée située
  entre deux poignées placées ne la déplace qu'entre elles.
- Changer de sens, modifier le texte de la ligne ou activer un pin libère
  toutes les poignées.
- `Ctrl + glisser` déplace toujours une seule poignée sans toucher aux
  autres, et cette poignée devient la nouvelle limite du calage
  proportionnel. `Ctrl + clic` sur une poignée, sans la déplacer, pose la
  limite là où elle est.

## Enregistrement accessible aux personnes malvoyantes

L’onglet Enregistrement se pilote entièrement au clavier et au lecteur
d’écran, en solo comme en session en ligne. Détails dans
[`RACCOURCIS_CLAVIER.md`](RACCOURCIS_CLAVIER.md).

- **Tout est lu avec son état.** Le nom d’une piste annonce aussi si elle est
  armée, si son son est coupé, si elle est en solo, son volume et son nombre
  de prises. Une prise dit qui l’a enregistrée, sur quelle piste, où elle
  commence et combien elle dure (« Prise de Alice, Piste 1, début 1 minute
  12 secondes, durée 3 secondes »). Un audio du projet dit sa durée et s’il
  est déjà placé ; un participant son rôle, son micro et s’il a le contrôle.
  Les boutons Muet, Solo, Armer et Réverbe disent « activé » ou « désactivé »,
  le volume est un vrai curseur.
- **Rien n’est hors d’atteinte.** Toutes les pistes, prises et audios sont
  dans l’ordre de `Tab`, même ceux qui ne tiennent pas à l’écran : la liste
  défile pour montrer celui qui a le focus.
- **Navigation aux flèches.** `↑` / `↓` passent au même bouton de la piste
  voisine, à la prise la plus proche de la piste voisine, à l’audio suivant ;
  `←` / `→` parcourent les boutons et les prises d’une même piste, règlent le
  volume par pas de 5 % et déplient ou replient les groupes d’audios.
  `Début` / `Fin` sur une prise amènent le curseur de lecture à son début ou
  à sa fin.
- **`Entrée` sélectionne la prise au curseur de lecture**, comme une ligne de
  la bande rythmo : celle de la piste du haut d’abord, puis, en appuyant de
  nouveau au même endroit, celle de la piste suivante. La prise est annoncée,
  ou « Aucune prise au curseur de lecture ».
- **Ce qui demandait la souris se fait au clavier.** `Ctrl + V` place l’audio
  sélectionné au curseur de lecture, sur la piste armée (avant, il fallait le
  glisser sur une piste). `Ctrl + Maj + ←` / `→` décalent les prises
  sélectionnées d’une image, `Ctrl + Maj + ↑` / `↓` les passent sur la piste
  voisine, `Ctrl + K` coupe la prise sélectionnée au curseur de lecture,
  `Suppr` supprime la prise ou l’audio qui a le focus.
- **Menu des audios.** Le clic droit sur un audio (ou `Maj + F10`) propose
  « Placer au curseur de lecture », « Envoyer vers › Voicelines » et
  « Supprimer l’audio », et se parcourt aux flèches.
- **Chaque action est annoncée** : outil choisi, piste ajoutée, supprimée ou
  renommée, son coupé ou rétabli, piste armée, volume, prise sélectionnée,
  déplacée, coupée ou supprimée, audio placé. Le renommage d’une piste
  annonce le nom à modifier et ses touches, et son annulation est annoncée.
  Quand une action est impossible, la raison est dite (« Aucune prise
  sélectionnée », « Armez d’abord une piste pour enregistrer »…) : le bouton
  d’enregistrement reste atteignable même sans piste armée.
- **Session en ligne.** Les comédiens entendent le compte à rebours lancé par
  le DA, pas seulement les bips. La fenêtre de transfert de projet se
  parcourt au clavier et annonce chacun de ses boutons.
- **Pour la basse vision.** Le contour du focus est plus épais et doublé
  (bleu et blanc) sur les contrôles de l’enregistrement ; les boutons actifs
  (Muet, Solo, Armer, Réverbe, outil), la prise et l’audio sélectionnés ont
  une bordure claire épaisse, et un audio déjà placé porte un repère vert à
  gauche : l’état ne dépend plus des couleurs seules. Les textes des pistes,
  des prises, des audios et des participants sont plus grands, et
  « Enregistrement en cours » est cerné de noir pour rester lisible sur
  l’image.
- Le panneau des raccourcis affiche ceux de l’enregistrement quand une prise
  ou un audio est sélectionné.

# 5.4.0

## Comic Dubs Studio

Le mode Comic Dubs devient un vrai petit studio de doublage de BD : on y
dessine les bulles, on place les plans de caméra case par case, on anime, on
enregistre et on mixe voix, bruitages et musique, et l’aperçu montre
exactement la vidéo exportée.

### Plans caméra

- Les cadrages sont désormais des **plans** posés sur la page, autant qu’on
  veut : outil « Plan caméra » dans la barre d’outils (ou « Tracer un plan »
  dans les réglages de la page), puis glisser sur la page. Le cadre suit
  toujours le format de la vidéo exportée ; un simple clic crée un plan
  standard.
- Les plans sont **toujours visibles** sur la page, numérotés (« PLAN 1 »,
  « PLAN 2 »…). On les déplace en glissant leur étiquette ou leur bord, on les
  redimensionne par les coins ; le plan sélectionné assombrit ce qu’il laisse
  hors champ. Le bouton « Plans » de la barre d’outils les masque.
- La vidéo enchaîne les plans dans leur ordre : la caméra rejoint le plan
  (mouvement fluide réglable ou coupe franche), marque une pause si besoin,
  puis ses bulles sont lues. Chaque bulle est montrée par le plus petit plan
  qui la contient (ou un plan choisi à la main) ; un plan sans bulle est tenu
  seul, comme un plan d’ensemble. « Page entière » crée un plan d’ensemble.
- Chaque bulle affiche le plan qui la montre (« P2 ») ; une bulle lue pendant
  un plan qui ne la cadre pas est signalée en rouge sur la page, dans
  l’inspecteur et sur la timeline, avec un bouton « Nouveau plan cadré sur
  cette bulle ».
- La timeline a une piste « Plans » ; cliquer un plan le sélectionne.

### Espace de travail

- Barre d’outils au-dessus de la planche avec de vraies icônes et des
  infobulles : Sélection, Bulle ronde, Bulle rectangulaire, Bulle de cri,
  Bulle de pensée, Cartouche de narration, Forme libre et Plan caméra. Après
  avoir tracé une bulle, on revient à la Sélection et on écrit directement son
  texte.
- Zoom sur la planche : `Ctrl` + molette autour du pointeur, molette ou clic
  molette pour se déplacer, bouton du zoom (« 100 % ») pour tout revoir.
- Panneau de gauche : miniatures des planches (monter, descendre, supprimer)
  et bibliothèque de sons (écouter, supprimer, glisser sur une bulle).
- L’inspecteur suit la sélection : **bulle** (Texte, Apparence, Animation,
  Son, Caméra, Organisation), **plan** (cadrage, arrivée, pause, bulles
  montrées, ordre, aperçu) ou, sans sélection, **Page** et **Projet**. Les
  sections se replient d’un clic.
- Réglages avec des curseurs (glisser, flèches du clavier ; un glissé = une
  seule étape d’annulation), des listes déroulantes qui montrent tous les
  choix, des interrupteurs et des boutons segmentés. Plus aucun symbole
  typographique en guise de bouton.
- Texte des bulles modifiable directement, avec un vrai curseur (flèches,
  `Début`, `Fin`, `Suppr`).
- Enregistrement à la chaîne : après chaque prise au micro, la bulle suivante
  sans voix est sélectionnée, prête à enregistrer.
- Une ligne d’aide contextuelle au-dessus de la timeline explique l’outil en
  cours.

### Effets

- Apparition des bulles (fondu, pop, zoom, glissements, chute avec rebond),
  bulle entière cachée avant son tour, machine à écrire ou mot par mot,
  emphase pendant la réplique (tremblement, pulsation, flottement,
  sautillement), effets d’écran (secousse, flash, impact), disparition après
  la réplique, sept préréglages de style, ombre portée, contour du texte.
- Transitions de page (fondu au noir, fondu enchaîné, glissements, zoom,
  flash), pause avant la première bulle, mouvement lent de caméra.
- Musique de fond en boucle, baissée automatiquement sous les voix, avec fondu
  de fin ; volume et délai de chaque voix, bruitage à l’apparition.

### Timeline et aperçu

- Pistes Pages, Plans, Bulles, Sons et Musique. Glisser sur la règle affiche
  l’image exacte du rendu ; cliquer un bloc sélectionne la bulle ou le plan
  (et change de page si besoin) ; bouton Lire / Arrêter.
- La lecture suit cette timeline : musique, voix et bruitages sont joués
  ensemble, et la bulle suivante n’arrive jamais avant la fin de la voix.

### Export

- Le rendu vidéo est recalculé image par image : plans caméra, transitions,
  animations, contours anti-crénelés, ombres, texte en italique et contour du
  texte sont identiques à l’aperçu. La musique de fond, les bruitages et le
  volume de chaque voix sont mixés dans la vidéo.
- Le rendu utilise tous les cœurs du processeur et les images identiques ne
  sont calculées qu’une fois.
- Le texte des bulles n’est plus légèrement écrasé verticalement à l’export.
- Nouveaux exports « Sous-titres des bulles (SRT) » et « Script de traduction
  (TXT) », et import d’un script : une ligne par bulle dans l’ordre de
  lecture, `# Page N` pour se recaler, `-` pour une bulle sans texte.

### Clavier et accessibilité

- Copier, couper, coller, dupliquer et déplacer les bulles au clavier, bulle
  précédente/suivante avec lecture de son texte, `Suppr` retire la bulle ou le
  plan sélectionné. Voir [`RACCOURCIS_CLAVIER.md`](RACCOURCIS_CLAVIER.md).
- Chaque outil, réglage, plan et bloc de timeline est un contrôle nommé pour
  les lecteurs d’écran ; les curseurs se règlent aux flèches et les listes
  s’ouvrent avec `Entrée`.

## Bande rythmo

- **Timecode** : le timecode courant s’affiche dans la barre d’outils de la
  bande rythmo. Les paramètres de la bande rythmo permettent de le masquer et
  de régler le timecode de départ du projet (par exemple `01:00:00:00`), qui
  s’applique aussi aux exports Référence et Grille. Un timecode de départ mal
  saisi est refusé et l’ancien est conservé.
- La hauteur de la waveform au-dessus de la bande se règle dans les paramètres
  de la bande rythmo.
- Les quatre pistes de la bande ont maintenant toutes la même hauteur, même
  quand une seule contient des lignes karaoké ou des effets de texte, et les
  répliques simples (sans effet de texte ni karaoké) occupent toute la hauteur
  de leur piste : elles ne paraissent plus écrasées. Le changement s’applique
  aussi à l’export vidéo.
- La piste survolée n’est plus éclairée en entier : seule la réplique sous le
  pointeur est mise en évidence.
- `Espace` lance ou met en pause la lecture dans la bande rythmo même quand un
  onglet, le panneau latéral ou l’arborescence des fichiers a le focus ; seule
  l’écriture dans un champ de texte la garde pour taper une espace.

## Édition des répliques

- **Dialogues liés** : sélectionnez plusieurs répliques, clic droit sur l’une
  d’elles, « Dialogues liés » puis « Lier les lignes sélectionnées ». Les
  répliques liées partagent ensuite tout sauf leur place sur la bande : texte,
  durée et poignées, calage des syllabes, points de synchronisation, note,
  karaoké, mise en forme et effets de texte. Modifier l’une modifie les autres,
  et `Ctrl + Z` défait le tout. La réplique cliquée donne son contenu aux
  autres au moment de la liaison. Le personnage (nom, couleur, comédiens)
  reste propre à chaque réplique, sauf si « Inclure le personnage » est coché
  dans le même sous-menu : l’option vaut pour le groupe et se change à tout
  moment. Une petite chaîne au bout de l’étiquette du personnage signale une
  réplique liée ; « Délier » défait le groupe sans toucher aux textes. Les
  liens sont propres à chaque langue de la bande, et une réplique copiée-collée
  n’est jamais liée d’office.
- `Ctrl + T` modifie la ligne sélectionnée **section par section**, sans
  toucher aux points de synchronisation ni au calage des syllabes du karaoké.
  Un petit champ de texte apparaît au-dessus de la première section (la
  section est surlignée sur la bande, un compteur « 1/4 » indique où l’on en
  est) ; on y tape le nouveau texte, `Entrée` remplace la section et passe à
  la suivante, `Échap` arrête. Les sections sont les parties comprises entre
  les points de synchronisation quand la ligne en a, sinon ses mots. Un mot
  remplacé par un mot du même nombre de syllabes garde exactement le calage
  de chaque syllabe ; avec un autre nombre, les nouvelles syllabes se
  partagent le temps de la section. Les limites avant et après la section et
  les syllabes des autres sections ne bougent pas. Chaque section validée est
  une étape de `Ctrl + Z`.

## Karaoké

- Écrire dans une réplique karaoké déjà calée ne défait plus le calage : les
  syllabes qu’on ne touche pas restent exactement à leur place. Un mot qui
  passe d’une à deux syllabes est coupé en deux dans l’espace qu’il occupait,
  et un mot ajouté prend sa place dans celui de la syllabe voisine. Une
  réplique karaoké jamais calée à la main part de la disposition affichée à
  l’écran. `Ctrl + Z` rétablit le texte et le calage d’avant.
- Effacer un mot (ou une syllabe) d’une réplique karaoké laisse sa case vide
  et réservée, encadrée par ses poignées : ce qu’on tape à sa place la reprend
  telle quelle, sans rien déplacer d’autre. Un second `Retour arrière` (ou
  `Suppr`) sur la case vide confirme la suppression : la case revient à la
  syllabe d’avant. Les poignées de la ligne ne se déplacent pas tant qu’une
  case est réservée ; quitter l’édition de la ligne confirme aussi.
- Changer la langue de syllabes d’une bande (ou dupliquer une bande dans une
  autre langue avec `Ctrl + L`) ne défait plus le calage des syllabes du
  karaoké ni celui des syllabes entre les points de synchronisation : le
  calage est converti sur-le-champ vers le découpage de la nouvelle langue,
  chaque lettre gardant sa place dans le temps ; seules les frontières de
  syllabes bougent. Les anciens projets dont le calage n’avait pas été
  converti sont rattrapés à l’affichage.
- Une réplique karaoké collée (`Ctrl + V`) après des répliques créées plus
  tard dans la chanson pouvait être sautée sur sa rangée : la réplique
  suivante s’y affichait à sa place, et les courtes répliques répétées
  (« Dis, dis, dis… ») n’étaient pas toutes visibles. Chaque rangée affiche
  de nouveau la réplique la plus proche, quel que soit l’ordre de création.

## Tableau des lignes et boucles

- Le tableau des lignes est maintenant trié dans l’ordre du film (par
  timecode), et non plus dans l’ordre de création.
- Les boucles sont numérotées dans l’ordre du film, et `Ctrl + Maj` + molette
  passe bien à la boucle suivante ou précédente dans le temps.
- Changer le rôle d’une phrase en bas du tableau des lignes ouvre le menu
  « Choisir un rôle » vers le haut, au lieu de le couper en bas de la fenêtre.

## Export

- Nouvel export « Scénario PDF » à donner aux comédiens (onglet Références de
  l’export) : le texte dans l’ordre du film, regroupé par boucle, avec tous
  les rôles ou un fichier par rôle qui ne contient que ses répliques.

## Enregistrement

- Chaque piste de l’espace Enregistrement a un bouton « Rev » qui ajoute une
  réverbération de chant à ses prises pendant l’écoute. Elle ne sert qu’à
  l’aperçu : le mixage exporté reste sans réverbération.

## Sessions en ligne

- Quitter une session la quitte vraiment : le serveur oublie le participant, et
  un doubleur qui revient (même sous un autre pseudo) n’apparaît plus plusieurs
  fois dans le téléchargement du projet, avec des réceptions restées en attente
  pour ses anciennes sessions. Les réceptions de prises en cours sont annulées.

## Installation

- L’installateur Windows (`Coquerythmo-Installer.exe`) joint aux versions
  installe de nouveau la version publiée : il installait jusqu’ici une
  ancienne version 4.1.0, qu’il fallait ensuite mettre à jour.

# 5.3.0

## Points de synchronisation

- Quand le pointeur reste un instant (1,5 s) sur l’endroit où un clic crée un point de synchronisation, une infobulle « Créer le point de synchronisation » l’indique.
- Un clic droit sur un point de synchronisation ouvre un menu contextuel : « Inclure le caractère courant » ou « Exclure le caractère courant » (comme `Maj`), et « Supprimer » (comme `Suppr`). Le menu se pilote aussi au clavier (flèches, `Entrée`, `Échap`).

## Formatage du texte

- Une barre de formatage apparaît au-dessus d’une ligne non karaoké dès qu’une partie de son texte est sélectionnée pendant l’édition. Elle met la sélection en gras, en italique, la souligne ou la barre. Un bouton allumé indique que tout le texte sélectionné porte déjà ce style ; un nouveau clic le retire. Le texte formaté garde l’étirement entre les points de synchronisation.
- Raccourcis : `Ctrl + B` (gras) et `Ctrl + U` (souligné) sur la sélection ou la ligne sélectionnée, et `Ctrl + I` (italique) pendant l’édition du texte. Hors édition, `Ctrl + I` ouvre toujours le panneau des lignes.
- Le formatage s’affiche dans la bande et dans l’export vidéo, suit le texte quand on le modifie, s’annule avec `Ctrl + Z`, est enregistré dans le projet et partagé en collaboration. Les exports SRT et ASS le conservent (balises `<b>`, `<i>`, `<u>`, `<s>` et `{\b1}`, `{\i1}`, `{\u1}`, `{\s1}`).
- Les lignes karaoké ne sont pas concernées.

## Style de la bande

- Nouvelle fenêtre « Style de la bande » (icône pinceau dans la barre du haut, à côté des paramètres du projet, ou `Ctrl + Maj + O`) : couleur du fond de la bande, couleur et épaisseur de la barre de lecture. Chaque réglage s’applique en direct sur la bande, qui reste visible à côté de la fenêtre ; « Annuler » ou `Échap` remet le style d’origine. Un clic sur la pastille de couleur ouvre un sélecteur (teinte, saturation, luminosité) ; la couleur se saisit aussi en code `#RRGGBB`.
- L’aperçu est désormais fidèle à l’export : la bande s’affiche dans l’éditeur avec exactement les couleurs de la vidéo exportée (fond, barre de lecture, texte, couleurs des personnages, ambiances, points karaoké), alors qu’elle paraissait jusqu’ici plus claire à l’écran. La barre de lecture de l’export accéléré par la carte graphique n’est plus d’un rouge différent.
- Presets de style : quatre presets fournis (Classique, Studio, Nuit, Contraste élevé), et les vôtres, enregistrés sous un nom et retrouvés dans tous les projets. Un style s’exporte en fichier `.coqstyle` à partager et s’importe depuis un tel fichier.
- Textures de point karaoké : le point qui rebondit sur les lignes karaoké peut être un rond (comme avant), un anneau, une étoile, un cœur, un losange, une note de musique, ou une image PNG de votre choix (« Image… »). Les formes prennent la couleur du personnage ; une image garde ses propres couleurs. Le choix vaut aussi pour le décompte avant la ligne, dans la bande comme dans l’export, et voyage avec les presets exportés. Avec « Deux images (sol / saut) », une image s’affiche quand le point touche le sol et une autre quand il est en l’air. Vous pouvez créer des groupes de points nommés (« Nouveau groupe ») ; puis clic droit sur une ligne karaoké → Karaoké pour donner au personnage le point par défaut, un groupe, ou un point qui n’appartient qu’à lui (forme ou image). Un personnage renommé garde son point ; supprimer un groupe remet ses personnages sur le point par défaut.
- Le style est enregistré dans le projet et commun à toutes ses langues.
- La bande de la vidéo exportée est maintenant identique à celle de l’éditeur : bords des lettres aussi nets (l’export accéléré par la carte graphique les assombrissait et les amincissait), marqueurs de boucle (barre, croix et numéro) et « out » (avec son libellé) aux mêmes couleurs et tailles, soulignement des répliques off ou partiellement off dans la couleur du texte, nom du personnage réduit pour tenir à côté de la réplique d’un autre personnage au lieu d’être rendu transparent, point karaoké rond avec son contour blanc et sous le texte, flèches de respiration avec leur pointe, icônes des comédiens encadrées et centrées comme dans l’éditeur. Dans l’éditeur, les marqueurs prennent les couleurs exactes de l’export.

## Barre de lecture

- La barre de lecture se déplace à la souris : on la saisit directement et on la glisse à gauche ou à droite, la bande suit en direct. Elle s’éclaircit au survol. Un double-clic dessus la recentre, `Échap` pendant le glissement annule. La position reste réglable dans les paramètres du projet.

## Export vidéo

- L’export d’une vidéo se fait maintenant en deux étapes. La fenêtre d’export sert d’abord à choisir les formats et réglages comme avant ; quand la vidéo MP4 est cochée, le bouton devient « Suivant… » et ouvre un écran « Mise en page de l’export ».
- Cet écran montre à droite un aperçu en direct de l’image exportée à la position de la tête de lecture : la vraie image de la vidéo et la vraie bande rythmo, dessinée comme dans le fichier final, sur le fond noir au format de sortie. La barre latérale de gauche règle la position X / Y, la largeur et la hauteur de la vidéo et de la bande séparément, ainsi que la taille de la bande et du texte karaoké ; l’aperçu suit chaque changement, et on peut aussi glisser la vidéo ou la bande à la souris dans l’aperçu.
- Un clic sur la vidéo ou la bande dans l’aperçu la sélectionne et affiche des poignées sur ses bords et ses coins (elles apparaissent aussi au survol, et le curseur change de forme) : tirer un bord gauche ou droit change la largeur, un bord haut ou bas la hauteur, un coin les deux. Le bord opposé reste en place pendant le redimensionnement ; le milieu du calque sert toujours à le déplacer.
- `Ctrl + Z` annule le dernier changement de mise en page (réglage, déplacement, redimensionnement — un glissement entier compte pour une seule étape —, réinitialisation, taille de la bande ou du texte karaoké) et `Ctrl + Y` ou `Ctrl + Maj + Z` le rétablit. Dans cet écran, ces raccourcis ne touchent jamais au projet ; le lecteur d’écran annonce ce qui a été rétabli.
- Le texte de la bande reste net quand on l’agrandit, même à 400 % : la bande est dessinée en vectoriel à la taille où elle sera affichée, puis seulement réduite, jamais étirée, dans l’aperçu comme dans la vidéo exportée.
- « Réinitialiser la mise en page » revient à la disposition habituelle (vidéo en haut, bande en bas sur toute la largeur), « Retour » revient aux réglages sans rien perdre, et « Lancer l’export » démarre l’export. Tout se fait aussi au clavier : `Tab` passe d’un réglage à l’autre, les flèches modifient la valeur, `Échap` revient à l’étape précédente.
- La mise en page (et les tailles de la bande et du texte karaoké) est mémorisée dans le projet, pour toutes ses langues : elle est retenue dès qu’on quitte cet écran, par « Retour », `Échap` ou « Lancer l’export », enregistrée avec le projet, et on la retrouve à la prochaine ouverture du projet sans avoir à la refaire. Avec une mise en page personnalisée, l’export passe par l’encodage logiciel.

# 5.2.1

## Édition

- Dans une réplique qui contient des points de synchronisation, écrire ou effacer du texte n’agit plus que sur la partie comprise entre deux limites : elle s’étire, tandis que les parties voisines et les points de synchronisation ne bougent pas. `Retour arrière` au début d’une partie et `Suppr` à sa fin ne mangent plus le texte de la partie voisine (une annonce vocale « Limite de synchronisation » l’indique).
- Sur une limite, le caret appartient à la partie d’où il vient : arrivé par la gauche (`→`), il écrit dans la partie de gauche ; arrivé par la droite (`←`), dans celle de droite. Un clic près d’une limite choisit la partie sous le pointeur. Sélectionner du texte à cheval sur une limite pour le supprimer ou le remplacer reste possible.
- Effacer la lettre ou la ponctuation à laquelle un point de synchronisation est accroché ne le décale plus d’un caractère, et taper une lettre identique à sa voisine juste à côté d’une limite ne dérègle plus la synchronisation.

## Détection

- Dans la palette `Alt + D` et les fiches de détection, une seconde description précise le cas d’usage et le placement de chaque indication, sans titre supplémentaire. Les panneaux adaptent leur taille au texte et à la largeur disponible, avec un retour automatique à la ligne. Le terme « dentale » est expliqué dans les indications concernées. Les explications s’appuient sur la charte de détection de l’UPAD et signalent les conventions variables ou propres à l’application. Elles sont également lues par le lecteur d’écran.
