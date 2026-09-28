# Comic Dubs Studio

Le mode Comic Dubs devient un vrai petit studio de doublage de BD : on y
dessine les bulles, on les anime, on cadre la caméra, on mixe voix, bruitages
et musique, et l’aperçu montre exactement la vidéo exportée.

## Outils de dessin

- Nouvelle boîte à outils à gauche de la planche : Sélection, Polygone libre,
  Bulle ronde, Bulle rectangulaire, Bulle de cri, Bulle de pensée, Cartouche de
  narration et Cadrage caméra. Glissez sur la page pour tracer une bulle ; un
  simple clic crée une bulle de taille standard.
- `Maj + clic` sur un bord ajoute un sommet, sur un sommet le retire. « Ajouter
  une queue » et « Arrondir » retouchent la forme ; les poses animées des
  sommets suivent automatiquement.

## Inspecteur à onglets

- **Texte** : taille, espacements, gras, italique, barré, souligné, alignement,
  couleur et contour du texte.
- **Style** : sept préréglages (Classique, Cri, Pensée, Chuchotement,
  Narration, Radio / écran, Onomatopée), couleur et épaisseur du contour, ombre
  portée, copier/coller le style, appliquer le style à toute la page,
  dupliquer.
- **Anim** : apparition (fondu, pop, zoom, glissements, chute avec rebond),
  bulle entière cachée avant son tour, révélation du texte en machine à écrire
  ou mot par mot, emphase pendant la réplique (tremblement, pulsation,
  flottement, sautillement), effets d’écran (secousse, flash, impact) et
  disparition après la réplique.
- **Caméra** : garder le cadrage, revenir à la page entière, zoomer sur la
  bulle ou cadrer une zone tracée avec l’outil Caméra, avec une durée de
  mouvement. Le texte et la voix arrivent quand la caméra est en place.
- **Son** : voix de la bulle, écoute, enregistrement direct au micro, volume,
  délai avant la voix, effet sonore à l’apparition et maintien après la
  réplique.
- **Page** : transition d’entrée (fondu au noir, fondu enchaîné, glissements,
  zoom, flash), pause avant la première bulle et mouvement lent de caméra.
- **Projet** : musique de fond (boucle, atténuation automatique sous les voix,
  fondu de fin), couleur de fond, vitesse de la machine à écrire, bilan des
  bulles sans voix ou sans texte, script et sous-titres.

## Timeline et aperçu

- Une timeline sous la planche montre les pages, les transitions, chaque bulle
  (déplacement caméra, réplique, voix, bruitage) et la musique avec ses
  atténuations. Cliquer ou glisser dessus affiche l’image exacte du rendu à cet
  instant ; cliquer une bulle la sélectionne.
- La lecture suit cette timeline : musique, voix et bruitages sont joués
  ensemble, et la bulle suivante n’arrive jamais avant la fin de la voix.
- L’aperçu est cadré au format de la vidéo exportée.

## Export

- Le rendu vidéo est recalculé image par image : caméra, transitions,
  animations, contours anti-crénelés, ombres, texte en italique et contour du
  texte sont identiques à l’aperçu. La musique de fond, les bruitages et le
  volume de chaque voix sont mixés dans la vidéo.
- Le rendu utilise tous les cœurs du processeur et les images identiques ne
  sont calculées qu’une fois.
- Le texte des bulles n’est plus légèrement écrasé verticalement à l’export.
- Nouveaux exports « Sous-titres des bulles (SRT) » et « Script de traduction
  (TXT) » dans le menu Export, et « Importer un script de traduction (TXT) »
  dans Imports : une ligne par bulle dans l’ordre de lecture, `# Page N` pour
  se recaler, `-` pour une bulle sans texte.

## Clavier et accessibilité

- Copier, couper, coller, dupliquer et déplacer les bulles au clavier, bulle
  précédente/suivante avec lecture de son texte, navigation d’une bulle à
  l’autre dans la timeline. Voir [`RACCOURCIS_CLAVIER.md`](RACCOURCIS_CLAVIER.md).
- Chaque outil, onglet, réglage et bloc de timeline est un contrôle nommé pour
  les lecteurs d’écran.

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


