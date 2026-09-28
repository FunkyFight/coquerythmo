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


