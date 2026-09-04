# 5.0.3

## Bugs

- Correction de la désynchronisation audio/vidéo après un rembobinage ou un déplacement dans la bande rythmo : les échantillons audio encore en mémoire sont désormais invalidés avant le démarrage du nouveau flux.
- Le file tree reste un panneau stable lorsque la souris sort de ses limites : l’état de survol et son animation sont correctement réinitialisés.
- Correction du chevauchement entre les badges du file tree et le nom des vidéos ou des audios ; le texte utilise maintenant uniquement l’espace disponible.
- Ajout de l’option « Rétablir le lien » pour les vidéos sources manquantes dans le file tree, avec sélection d’un nouveau fichier et sauvegarde du chemin restauré.
- Correction de l’affichage des sous-menus contextuels du file tree : ils apparaissent à côté du menu parent au lieu de le recouvrir.

## Raccourcis et édition

- Nouveau raccourci `Ctrl + Shift + V` : colle la ligne copiée en reprenant le dernier personnage utilisé sur la piste cible, au point de collage. Le personnage d’origine reste utilisé avec `Ctrl + V`.

## Performance

- Le déplacement dans la bande rythmo (scrub) ne fige plus l’interface, même en répétition intensive : le pas à pas image par image (`Ctrl + ←` / `Ctrl + →`) est désormais asynchrone et les déplacements rapprochés sont fusionnés en un seul décodage.
- Le flux audio n’est plus détruit puis recréé à chaque déplacement : seule la source de décodage est remplacée, ce qui supprime les à-coups liés à la réinitialisation audio pendant le scrub.
- La sauvegarde des projets `.coquerythmo` est nettement plus rapide : les médias sont copiés en parallèle sur tous les cœurs du processeur et les empreintes CRC-32/SHA-1 exploitent les instructions matérielles du processeur.
- La sauvegarde automatique (toutes les 60 secondes) s’effectue désormais en arrière-plan et ne bloque plus l’interface.
- Le chargement de projet, l’export et la création de proxy n’affichent plus de fenêtre modale bloquante : ils apparaissent sous forme de lignes de tâches discrètes en bas à droite, avec barre de progression, et l’interface reste utilisable pendant leur exécution.

## Affichage

- Nouveau panneau de contrôles en bas à gauche : il liste les raccourcis clavier réellement utilisables dans la situation courante (ligne sélectionnée, édition de texte, modale, détection…), avec une barre de défilement quand la liste dépasse. (BÊTA)
- Nouveau paramètre « Activer l’affichage des contrôles » dans les paramètres de l’application, activé par défaut.
- Quand une ligne de la bande rythmo est sélectionnée, la partie de la forme d’onde qu’elle couvre prend la couleur de son personnage.

- Remplacement de l'explorateur de média avec un file tree expérimental

## Mode enregistrement
- Correction de la synchronisation en boucle quand un doubleur tente de se connecter à une session trop lourde.
- Les doubleurs connectés reçoivent de nouveau correctement les changements de mute et de solo des pistes.
- Correction critique de l’écoute en session d’enregistrement en ligne : le DA et tous les doubleurs entendent désormais l’intégralité des audios présents sur la timeline, quelle que soit leur origine ou l’opération effectuée (prise fraîchement enregistrée, import externe, audio interne ou envoi depuis Voicelines). Les fichiers manquants sont également retransmis après une connexion ou une reconnexion avant le démarrage de la lecture.

## Nouveau mode Voicelines

- Travaillez sur des voix sans vidéo ni bande rythmo.
- Importez plusieurs fichiers audio, puis passez librement de l’un à l’autre.
- Créez, déplacez et redimensionnez les zones de découpe directement sur la forme d’onde.
- Nommez les zones manuellement ou automatiquement.
- Détectez les silences en un clic pour générer les zones.
- Écoutez une zone isolée, zoomez et naviguez précisément dans l’audio.
- Exportez une zone ou toutes les zones au format OGG, avec un fichier audio distinct par voiceline.
- Sauvegardez et rechargez les sources, zones et réglages des sessions Voicelines.
- Profitez de l’annulation, du rétablissement, du glisser-déposer et de la navigation clavier.
- La barre de menus Voicelines se concentre désormais sur le projet et l’export audio.
- En session serveur, l’envoi de voicelines vers le mode Enregistrement est désactivé.

## Nouveau mode Comic Dubs

- Créez un doublage de bande dessinée sans vidéo ni bande rythmo.
- Importez plusieurs pages illustrées et plusieurs fichiers audio.
- Réorganisez, sélectionnez ou supprimez les pages depuis leur liste.
- Dessinez des bulles polygonales directement sur chaque page.
- Modifiez le texte, la couleur, la taille et la forme de chaque bulle.
- Réorganisez les bulles pour définir leur ordre de lecture.
- Repérez immédiatement l’ordre de lecture et les bulles sonorisées grâce aux indicateurs affichés en mode édition.
- Associez une piste audio à chaque bulle et prévisualisez leur enchaînement.
- Réglez la police, la taille de texte par défaut et les durées entre bulles et pages.
- Exportez le doublage complet depuis un menu dédié à la vidéo MP4, avec les pages, les textes et les voix.
- Annulez et rétablissez les modifications propres au mode Comic Dubs.
- Sauvegardez et rechargez les pages, audios, bulles et paramètres dans le projet.

## Paramètres et interface

- Nouveau menu déroulant de polices partagé par la bande rythmo et Comic Dubs.
- Chaque nom de police est affiché directement avec la police correspondante.
- Les paramètres de la bande rythmo regroupent désormais la police, la vitesse de défilement, le décalage de la barre de lecture, la version instrumentale et les options d’affichage.
- Les paramètres généraux de l’application sont simplifiés à la langue et au dossier temporaire.
- Correction durable de l’ordre d’affichage des panneaux, menus et fenêtres modales.
- Import de fichiers audio ajouté au mode Enregistrement.

## Corrections

- La lecture démarre désormais sans attente désagréable après un déplacement dans la bande rythmo.
- Une vidéo liée à la bande rythmo n’est plus affichée dans le mode Comic Dubs.
- Les projets Comic Dubs et Voicelines peuvent être sauvegardés sans vidéo source.
- Les archives de projet conservent les données et médias des nouveaux modes.
- Correction du chevauchement entre le libellé et le sélecteur de police dans les paramètres Comic Dubs.
