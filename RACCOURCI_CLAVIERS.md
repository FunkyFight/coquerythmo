# Raccourcis clavier de Coquerythmo

Cette liste décrit les raccourcis effectivement câblés dans le routeur d’entrée. Les actions sont sémantiques : AccessKit annonce leur résultat, jamais le nom brut des touches à la place de l’action.

## Lecture et navigation de la bande rythmo

| Raccourci | Action |
|---|---|
| `Espace` | Lecture / pause |
| `Ctrl` + `Espace` | Préécouter le signe de détection sélectionné avec l’image, l’audio et un bip renforcé |
| `Q` maintenu | Défilement vers la gauche |
| `D` maintenu | Défilement vers la droite |
| `Ctrl` + `Flèche gauche` | Image précédente |
| `Ctrl` + `Flèche droite` | Image suivante |
| `Maj` + `Flèche gauche` | Réplique précédente |
| `Maj` + `Flèche droite` | Réplique suivante |
| `Ctrl` + `Maj` + `Flèche gauche` | Décaler la sélection d’une image vers la gauche |
| `Ctrl` + `Maj` + `Flèche droite` | Décaler la sélection d’une image vers la droite |
| `Flèche haut` / `Flèche bas` | Déplacer la réplique sélectionnée sur la piste voisine |
| `Entrée` | Sélectionner la réplique au curseur de lecture |
| `Échap` | Quitter la sélection ou fermer l’interaction courante |
| `Suppr` | Supprimer la sélection, y compris une détection ou un marqueur de production |

## Détection et états de ligne

| Raccourci | Action |
|---|---|
| `Alt` + `D` | Ouvrir la palette de détection à la position survolée |
| `Tab` dans la palette | Atteindre la rangée OFF, de dos, début d’ambiance et fin d’ambiance |
| `Maj` + `Tab` | Revenir au contrôle précédent |
| `Flèche gauche` / `Flèche droite` | Parcourir la rangée active |
| `Entrée` | Activer ou désactiver l’état sélectionné |

## Marqueurs de production non exportés

| Raccourci | Action |
|---|---|
| `Alt` + `Maj` + `1` | Ajouter un marqueur `START` à l’image courante |
| `Alt` + `Maj` + `2` | Ajouter un marqueur `1000` ; il joue un bip de 1000 Hz au franchissement en lecture |
| `Alt` + `Maj` + `3` | Ajouter un marqueur première image `P.I` |
| `Alt` + `Maj` + `4` | Ajouter un marqueur dernière image `D.I` |

Ces quatre marqueurs sont visibles et déplaçables dans l’éditeur, mais ne sont ni dessinés ni sonorisés dans les exports.

## Édition des répliques

| Raccourci | Action |
|---|---|
| `I` | Placer le début de la réplique sélectionnée au curseur |
| `O` | Placer la fin de la réplique sélectionnée au curseur |
| `T` | Éditer le texte de la réplique sélectionnée |
| `P` | Éditer le personnage de la réplique sélectionnée |
| `Ctrl` + `K` | Scinder le dialogue |
| `Ctrl` + `A` | Tout sélectionner dans le contexte actif |
| `Ctrl` + `C` / `X` / `V` | Copier / couper / coller |
| `Ctrl` + `Z` | Annuler |
| `Ctrl` + `Maj` + `Z` | Rétablir |

## Pavé numérique

| Raccourci | Action |
|---|---|
| `Pavé 1` | Ajouter un début de boucle |
| `Pavé 2` | Ajouter un OUT |
| `Pavé 3` | Ajouter un changement de scène |
| `Pavé 4` | Ouvrir les respirations |
| `Pavé 5` | Ouvrir les réactions |
| `Pavé 6` | Ajouter une note |
| `Pavé 7` | Ajouter une liaison à gauche |
| `Pavé 8` | Ajouter une liaison à droite |
| `Pavé 9` | Activer ou désactiver le karaoké pour la sélection |
| `Ctrl` + `Pavé 1…4` | Créer une réplique sur la piste correspondante |
| `Maj` + `Pavé -` | Couper ou rétabl le son |

## Projet et interface

| Raccourci | Action |
|---|---|
| `Ctrl` + `S` | Sauvegarde rapide |
| `Ctrl` + `N` | Nouveau projet |
| `Ctrl` + `Suppr` | Fermer le projet |
| `Ctrl` + `R` | Projets récents |
| `Ctrl` + `Maj` + `R` | Restaurer une sauvegarde |
| `Ctrl` + `Maj` + `I` | Importer des sous-titres |
| `Ctrl` + `M` | Ouvrir l’export |
| `Ctrl` + `Maj` + `P` | Ouvrir la création de proxy |
| `Ctrl` + `O` | Ouvrir les paramètres du projet |
| `Ctrl` + `I` | Ouvrir le panneau des répliques |
| `Ctrl` + `P` | Ouvrir le panneau des rôles |
| `Ctrl` + `Tab` | Basculer entre l’audio source et l’instrumental |
| `Ctrl` + `Maj` + `N` | Activer ou désactiver le lecteur d’écran intégré |

## Souris

| Interaction | Action |
|---|---|
| Glisser un signe de détection | Changer son image ou sa piste |
| `Maj` + glisser un signe | Changer sa piste en conservant son image |
| Glisser un point de synchronisation | Déplacer uniquement la limite entre ses deux segments voisins |
| Glisser un marqueur `START`, `1000`, `P.I` ou `D.I` | Changer son image |
| Clic molette + glisser | Déplacer la vue |
