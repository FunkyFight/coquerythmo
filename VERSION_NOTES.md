# 5.0.3

## Performance

- Le déplacement dans la bande rythmo (scrub) ne fige plus l’interface, même en répétition intensive : le pas à pas image par image (`Ctrl + ←` / `Ctrl + →`) est désormais asynchrone et les déplacements rapprochés sont fusionnés en un seul décodage.
- Le flux audio n’est plus détruit puis recréé à chaque déplacement : seule la source de décodage est remplacée, ce qui supprime les à-coups liés à la réinitialisation audio pendant le scrub.
- La sauvegarde des projets `.coquerythmo` est nettement plus rapide : les médias sont copiés en parallèle sur tous les cœurs du processeur et les empreintes CRC-32/SHA-1 exploitent les instructions matérielles du processeur.
- La sauvegarde automatique (toutes les 60 secondes) s’effectue désormais en arrière-plan et ne bloque plus l’interface.

## Affichage

- Nouveau panneau de contrôles en bas à gauche : il liste les raccourcis clavier réellement utilisables dans la situation courante (ligne sélectionnée, édition de texte, modale, détection…), avec une barre de défilement quand la liste dépasse. (BÊTA)
- Nouveau paramètre « Activer l’affichage des contrôles » dans les paramètres de l’application, activé par défaut.
- Quand une ligne de la bande rythmo est sélectionnée, la partie de la forme d’onde qu’elle couvre prend la couleur de son personnage.

## Mode enregistrement
- Correction de la synchronisation en boucle quand un doubleur tente de se connecter à une session trop lourde.

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
