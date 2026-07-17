# coquerythmo-releases
Bande rythmo simple, épurée et optimisée. Tout ça, gratuitement.

## Version 3.5.4

Cette version améliore fortement l’accessibilité clavier et la lecture vocale.
Sous Windows, les annonces sont transmises au système d’accessibilité via
AccessKit. Les raccourcis nommés annoncent uniquement leur action, sans lire
la combinaison de touches ; les listes
annoncent leur premier élément puis leur état « réduite » à la fermeture, et
les modales annoncent leur fermeture.

Tous les toasts sont vocalisés sur le canal prioritaire. Les exportations et
créations de proxy sont exposées à AccessKit comme des barres de progression ;
l’opération en cours et son pourcentage sont annoncés toutes les minutes sur
ce canal. Leur démarrage et leur annulation avec `Échap` sont aussi annoncés
en priorité.

Avec la lecture vocale active sous Windows, les sélecteurs de fichiers utilisent
l’Explorateur Windows natif. Les projets `.coquerythmo` peuvent également être
ouverts avec Coquerythmo depuis le menu « Ouvrir avec » de Windows.

La liste complète des raccourcis est disponible dans
[`RACCOURCIS_CLAVIER.md`](RACCOURCIS_CLAVIER.md).

## Tutoriels
[![Tutoriel français](https://img.youtube.com/vi/m_SpxXRjvmg/maxresdefault.jpg)](https://www.youtube.com/watch?v=m_SpxXRjvmg)
[Tutoriel français](https://www.youtube.com/watch?v=m_SpxXRjvmg)

[![V3.4.0 (Pro Tools)](https://img.youtube.com/vi/nKFU_zl7Duc/maxresdefault.jpg)](https://www.youtube.com/watch?v=nKFU_zl7Duc)
[V3.4.0](https://www.youtube.com/watch?v=nKFU_zl7Duc)

[![Tutoriel français BANDE RYTHMO ENSEMBLE](https://img.youtube.com/vi/rJi6mt2Jax4/maxresdefault.jpg)](https://www.youtube.com/watch?v=rJi6mt2Jax4)
[Tutoriel français BANDE RYTHMO ENSEMBLE](https://www.youtube.com/watch?v=rJi6mt2Jax4)

## Raccourcis clavier

Les raccourcis sont interprétés selon le contexte actif. Un champ texte est
prioritaire sur les raccourcis de l’espace de travail. Dans une modale, le
focus reste enfermé dans la modale jusqu’à sa fermeture.

Un raccourci annonce uniquement le nom de son action lorsqu’il en possède un.
La combinaison de touches n’est jamais lue et aucun nom de touche ne sert de
fallback. Toutes les annonces produites par un raccourci utilisant `Ctrl`, y
compris avec `Maj`, sont prioritaires.

### Navigation commune

| Raccourci | Action | Contexte |
|---|---|---|
| `Tab` | Focus suivant | Contrôles, listes, menus et modales |
| `Maj + Tab` | Focus précédent | Contrôles, listes, menus et modales |
| `Entrée` / `Espace` | Activer le contrôle focalisé | Contrôles, listes, menus et modales |
| `Échap` | Fermer le niveau courant ou annuler | Modales, menus et éditeurs |
| `↑` / `↓` | Parcourir une liste ou modifier une valeur | Contrôles focalisés |
| `←` / `→` | Parcourir ou régler le contrôle | Contrôles focalisés |
| `Début` / `Fin` | Aller au premier/dernier élément | Listes et contrôles |
| `Page haut` / `Page bas` | Parcourir par page | Listes |
| `Maj + F10` | Ouvrir le menu contextuel de l’élément focalisé | Contrôles et lignes |

### Barre supérieure et projet

| Raccourci | Action |
|---|---|
| `Ctrl + S` | Sauvegarde rapide |
| `Ctrl + Maj + I` | Importer un sous-titrage (`.coquerythmo`, `.json`, `.srt`, `.ass`, `.detx`) |
| `Ctrl + Maj + R` | Restaurer la dernière sauvegarde disponible |
| `Ctrl + R` | Ouvrir les projets récents |
| `Ctrl + Suppr` | Fermer le projet |
| `Ctrl + M` | Ouvrir l’export du projet |
| `Ctrl + Alt + S` | Afficher l’avertissement et ouvrir le mode Studio |
| `Ctrl + Maj + P` | Ouvrir la création d’un proxy |
| `Ctrl + O` | Ouvrir les paramètres du projet |
| `Ctrl + N` | Créer un nouveau projet |
| `Ctrl + I` | Ouvrir le panneau des lignes |
| `Ctrl + P` | Ouvrir le panneau des rôles |
| `Ctrl + L` | Ouvrir le sélecteur de langues (hors modale) |
| `F5` | Afficher l’avertissement Studio lorsqu’une vidéo est chargée |

Dans la liste des projets récents : `↑`/`↓` parcourent les projets, `Entrée`
ouvre le projet, `Suppr` le retire de la liste et `Échap` ferme la liste.

Dans les paramètres du projet, l’option « Définir la couleur du texte à la
couleur de l’étiquette de personnage » applique la couleur de l’étiquette au
texte des lignes défilantes non karaoké.

### Outils et lecture

| Raccourci | Action |
|---|---|
| `Ctrl + ←` / `Ctrl + →` | Image précédente/suivante |
| `Numpad 1` | Ajouter une boucle |
| `Numpad 2` | Ajouter un point Out |
| `Numpad 3` | Ajouter un changement de scène |
| `Numpad 4` | Ouvrir les respirations |
| `Numpad 5` | Ouvrir les réactions |
| `Numpad 6` | Ajouter une note |
| `Numpad 7` / `Numpad 8` | Ajouter une liaison à gauche/droite |
| `Numpad 9` | Activer/désactiver le karaoké |
| `Ctrl + Numpad 1` à `Ctrl + Numpad 4` | Créer une ligne sur la piste 1 à 4 |
| `C` | Outil Sélection |
| `Ctrl + D` | Outil Dessin |
| `Maj + ↑` / `Maj + ↓` | Augmenter/diminuer le volume par pas de 5 % |
| `Maj + Numpad -` | Activer/désactiver le muet |
| `Espace` | Lecture/pause |
| `Ctrl + Tab` | Basculer entre l’audio original et l’audio instrumental |

### Bande rythmo

| Raccourci | Action |
|---|---|
| `Entrée` | Sélectionner successivement les lignes présentes au frame courant |
| `↑` / `↓` (ligne sélectionnée) | Déplacer la ligne vers la piste précédente/suivante |
| `Maj + ←` / `Maj + →` | Parcourir toutes les lignes ; sans sélection préalable, commencer par la ligne la plus proche de la tête de lecture, puis annoncer le personnage, le dialogue et la piste |
| `Ctrl + Maj + ←` / `Ctrl + Maj + →` | Décaler la ou les lignes sélectionnées d'une frame vers la gauche/droite |
| `Échap` | Annuler la sélection actuelle de lignes |
| `I` / `O` | Fixer le début/la fin de la ligne sélectionnée |
| `Q` / `D` (maintenir) | Déplacer continuellement la timeline à gauche/droite |
| `T` | Modifier le texte de la ligne sélectionnée |
| `P` | Modifier le personnage de la ligne sélectionnée |
| `Suppr` | Supprimer la ligne sélectionnée |
| `Ctrl + C` | Copier la ligne sélectionnée |
| `Ctrl + X` | Couper la ligne sélectionnée |
| `Ctrl + V` | Coller la ligne sur la piste sous le curseur de souris |

Si la souris est hors de la bande rythmo, `Ctrl + V` utilise la piste clavier
active. Pendant l’édition d’une ligne ou d’un personnage, les flèches déplacent
le caret et ne modifient pas le volume.

### Export

| Raccourci | Action |
|---|---|
| `Tab` / `Maj + Tab` | Parcourir tous les réglages, formats, langues et boutons |
| `↑` / `↓` sur le menu des pages | Passer entre Vidéo, Sous-titres, Audio et Références |
| `←` / `→` sur le menu des pages | Passer entre les pages d’export |
| `↑` / `↓` sur un réglage | Modifier sa valeur |
| `Entrée` / `Espace` | Activer un format, une langue ou un bouton |
| `Échap` | Fermer l’export |

Les champs largeur et hauteur acceptent les chiffres, `Retour arrière` et
`Entrée` termine leur édition.

Dans la création de proxy, `Tab` / `Maj + Tab` parcourent la résolution, la
qualité et le bouton de création. Les flèches règlent la valeur focalisée,
`Entrée` / `Espace` l’active et `Échap` ferme la modale.

### Champs texte et modales

| Raccourci | Action |
|---|---|
| `Ctrl + A` | Tout sélectionner |
| `Ctrl + Maj + ↑` | Sélectionner tout le texte du champ en édition |
| `Ctrl + C` / `Ctrl + X` / `Ctrl + V` | Copier/couper/coller le texte |
| `Ctrl + Z` | Annuler |
| `Ctrl + Maj + Z` | Rétablir |
| `←` / `→` | Déplacer le caret |
| `Maj + ←` / `Maj + →` | Étendre la sélection |
| `Ctrl + Maj + ←` / `Ctrl + Maj + →` | Étendre la sélection mot par mot |
| `Début` / `Fin` | Aller au début/à la fin du champ |
| `↑` / `↓` | Parcourir les suggestions ou les éléments de liste |
| `Retour arrière` / `Suppr` | Supprimer le caractère précédent/suivant |

Dans l’Explorateur de fichiers, `Alt + ←`/`Alt + →` parcourent l’historique,
`Retour arrière` remonte au dossier parent et `Entrée` ouvre le fichier ou
valide l’action principale.

### Lecture vocale interne

| Raccourci | Action |
|---|---|
| `Ctrl + Maj + N` | Activer/désactiver la lecture vocale interne |
| `Ctrl` | Interrompre la lecture vocale en cours |
| `Maj` | Reprendre la lecture vocale interrompue |

Chaque changement de focus, sélection, activation, valeur et résultat d’action
est annoncé. Une ligne karaoké est identifiée après son numéro de piste. Les
raccourcis sans nom sémantique, ainsi que `Q` et `D`, restent silencieux.

L’ouverture d’une liste annonce son premier élément disponible. Sa fermeture
annonce que la liste est réduite. La fermeture d’une modale est également
annoncée.

Sur Linux, la lecture vocale interne reste indisponible et le raccourci affiche
un message localisé.
