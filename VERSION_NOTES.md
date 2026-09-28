# Comic Dubs Studio

Le mode Comic Dubs devient un vrai petit studio de doublage de BD : on y
dessine les bulles, on place les plans de caméra case par case, on anime, on
enregistre et on mixe voix, bruitages et musique, et l’aperçu montre
exactement la vidéo exportée.

## Plans caméra

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

## Espace de travail

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

## Effets

- Apparition des bulles (fondu, pop, zoom, glissements, chute avec rebond),
  bulle entière cachée avant son tour, machine à écrire ou mot par mot,
  emphase pendant la réplique (tremblement, pulsation, flottement,
  sautillement), effets d’écran (secousse, flash, impact), disparition après
  la réplique, sept préréglages de style, ombre portée, contour du texte.
- Transitions de page (fondu au noir, fondu enchaîné, glissements, zoom,
  flash), pause avant la première bulle, mouvement lent de caméra.
- Musique de fond en boucle, baissée automatiquement sous les voix, avec fondu
  de fin ; volume et délai de chaque voix, bruitage à l’apparition.

## Timeline et aperçu

- Pistes Pages, Plans, Bulles, Sons et Musique. Glisser sur la règle affiche
  l’image exacte du rendu ; cliquer un bloc sélectionne la bulle ou le plan
  (et change de page si besoin) ; bouton Lire / Arrêter.
- La lecture suit cette timeline : musique, voix et bruitages sont joués
  ensemble, et la bulle suivante n’arrive jamais avant la fin de la voix.

## Export

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

## Clavier et accessibilité

- Copier, couper, coller, dupliquer et déplacer les bulles au clavier, bulle
  précédente/suivante avec lecture de son texte, `Suppr` retire la bulle ou le
  plan sélectionné. Voir [`RACCOURCIS_CLAVIER.md`](RACCOURCIS_CLAVIER.md).
- Chaque outil, réglage, plan et bloc de timeline est un contrôle nommé pour
  les lecteurs d’écran ; les curseurs se règlent aux flèches et les listes
  s’ouvrent avec `Entrée`.

# 5.2.0

## Communauté

- Le bouton « Discord » affiche désormais un avertissement avant d’ouvrir le serveur : Coquerythmo est entièrement gratuit et libre, mais son créateur ne peut pas assurer le support de tout le monde. La communauté se fera un plaisir de répondre à vos questions, et un support prioritaire est disponible à un tarif variant selon la structure (tarifs à venir).
- À la fin d’un export, une fenêtre confirme qu’il est terminé, à la place de la courte notification d’avant. Elle rappelle aussi que Coquerythmo est gratuit et compte sur ses utilisateurs : s’il vous a servi à produire un média, quel qu’il soit, merci de le créditer au générique ou dans la description. `Entrée`, `Échap` ou « OK » ferment la fenêtre.

## Export

- Les bips qui accompagnaient la progression d’un export ou de la création d’un proxy ont été retirés. La lecture vocale interne (`Ctrl + Maj + N`) continue d’annoncer l’avancement.

## Bugs

- Importer la vidéo après la bande rythmo (`.detx`, `.json`, `.srt` ou `.ass`) ne désynchronise plus les répliques. Elles s’étiraient d’environ 20 % sur une vidéo à 25 i/s, avec un décalage qui grandissait au fil du film. Elles sont maintenant recalées sur la cadence de la vidéo dès son chargement, exactement comme si la vidéo avait été importée en premier. L’historique d’annulation repart à zéro à ce moment-là.
- Les timecodes des fichiers `.detx` sont lus correctement sur les vidéos à 23,976 et 29,97 i/s. Avant, les répliques y glissaient d’une image par seconde.
- Il est de nouveau possible d’ajouter une bande rythmo depuis l’explorateur de fichiers (`Ctrl + L`) : un clic droit sur « Bandes rythmo » ou sur une bande affiche « Ajouter une bande rythmo ». La nouvelle bande duplique la bande active, reprend sa langue de découpe et passe directement en renommage.
- Cliquer en dehors de l’explorateur de fichiers (`Ctrl + L`) lui retire le focus clavier : les touches (Espace, flèches, Suppr, Échap…) agissent de nouveau sur la bande rythmo. Un clic dans l’explorateur lui rend le focus, et un renommage en cours est validé quand on clique ailleurs.

## Édition

- La suppression d’une bande rythmo peut être annulée avec `Ctrl + Z` : la bande revient à sa place, avec son contenu et sa sélection d’export, et redevient active si elle l’était. `Ctrl + Shift + Z` la supprime à nouveau.

## Lecture

- Nouvelle lecture à l’envers : `Maj + Espace`, ou le bouton ◀ placé à côté de Lecture, lit la vidéo en arrière à vitesse normale et avec le son. C’est pratique pour vérifier le calage des débuts de phrase. `Espace` ou `Maj + Espace` met en pause. Les vidéos compressées avec de longs groupes d’images (H.264, H.265…) peuvent saccader : avec un proxy, la lecture reste fluide. Elle n’est pas disponible pendant une session d’enregistrement partagée.

## Enregistrement

- La bande rythmo du mode enregistrement répond aux mêmes raccourcis de navigation que le mode bande rythmo : `Ctrl + ←/→` recule ou avance d’une image, `Q`/`D` font défiler la bande, `Maj + ↑/↓` règle le volume et `Maj + −` (pavé numérique) coupe le son.

## Paramètres

- Nouvelle option « Masquer les indications de formatage » dans les Paramètres. Elle cache les avertissements et les non-conformités de la bande rythmo (par exemple « ce personnage apparaît sur plusieurs lignes »), à l’écran comme dans les descriptions lues par le lecteur d’écran.
- Les options à cocher des Paramètres et des paramètres du projet, dont « Afficher les couloirs d’émotion de texte » et « Activer l’affichage des contrôles », sont de vraies cases à cocher, avec une coche. Avant, elles ressemblaient à des boutons.


