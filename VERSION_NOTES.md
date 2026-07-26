# 3.7.1

## Système d’émotion de texte

- Ajout d’un système permettant d’appliquer une émotion à une ligne entière ou à une sélection de texte depuis le menu contextuel.
- Les émotions sont organisées par catégories et proposent plusieurs intensités, notamment pour la colère, la joie, la peur, la tristesse et la tendresse.
- Le texte émotionnel est animé dans la bande rythmo et accompagné d’un couloir de lecture, affichable ou masquable dans les paramètres du projet.
- Le menu contextuel et le réglage des couloirs sont entièrement navigables au clavier et vocalisés par les lecteurs d’écran.
- Ajout des effets Pendulum, Swing, Yay, Bounce, Slide, Oscillation, Wave, Shake et Wiggle.
- Les Text Emotions sont conservées dans les sauvegardes JSON et `.coquerythmo`.

## Bande rythmo

- Les étiquettes de personnages dans les exports CPU et GPU conservent désormais les proportions, la taille et l’espacement de l’interface.
- Le décalage de la barre de lecture est appliqué dans le calcul central des coordonnées.
- Les lignes, overlays, marqueurs, formes d’onde, dessins, détections, aperçus fantômes et raccourcis respectent désormais ce décalage.
- Les marqueurs restent visibles et alignés avec les lignes dans les exports CPU et GPU, même lorsque la barre de lecture est décalée.
- Les badges de personnages entrent à l’écran sans délai, nom compris.
- Un badge masqué par une ligne proche reste masqué lorsque cette ligne quitte l’écran.
- Le réglage du décalage est persisté dans la configuration.
- Le déplacement des points de synchronisation conserve désormais l’alignement avec leur position affichée, sans téléportation au début du drag.

## Sauvegarde

- Correction des erreurs de sauvegarde causées par un FPS de journal différent du FPS du projet.
- Le checkpoint du journal est reconstruit depuis le projet courant lorsque son FPS est obsolète.

## Accessibilité

- Nombreuses améliorations de navigation, d’annonces et de cohérence des contrôles.

# À faire

- Poignées karaoké sur toute la hauteur de leur ligne.
- Exporter et importer les nodes.
