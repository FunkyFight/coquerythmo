# Raccourcis clavier

Les raccourcis sont interprétés selon le contexte actif. Un champ texte est
prioritaire sur les raccourcis de l’espace de travail. Dans une modale, le
focus reste enfermé dans la modale jusqu’à sa fermeture.

Pour les personnes malvoyantes qui utilisent un logiciel de lecture d’écran :
un raccourci annonce uniquement le nom de son action lorsqu’il en possède un.
La combinaison de touches n’est jamais lue et aucun nom de touche ne sert de
fallback. Sous Windows, les annonces sont transmises au système
d’accessibilité via AccessKit. Les annonces produites avec `Ctrl`, y compris
avec `Maj`, sont prioritaires.

Les toasts utilisent également le canal prioritaire. Pendant une exportation ou
la création d’un proxy, la progression est exposée comme une barre de
progression au système d’accessibilité. L’opération et son pourcentage sont
annoncés toutes les minutes. `Échap` annule l’opération en cours et
l’annulation est annoncée en priorité.

## Navigation commune

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

## Barre supérieure et projet

| Raccourci | Action |
|---|---|
| `Ctrl + S` | Sauvegarde rapide |
| `Ctrl + Maj + I` | Importer un sous-titrage (`.coquerythmo`, `.json`, `.srt`, `.ass`, `.detx`) |
| `Ctrl + Maj + R` | Restaurer la dernière sauvegarde disponible |
| `Ctrl + R` | Ouvrir les projets récents |
| `Ctrl + Suppr` | Fermer le projet |
| `Ctrl + M` | Ouvrir l’export du projet |
| `Ctrl + Maj + P` | Ouvrir la création d’un proxy |
| `Ctrl + O` | Ouvrir les paramètres du projet |
| `Ctrl + N` | Créer un nouveau projet |
| `Ctrl + I` | Ouvrir le panneau des lignes |
| `Ctrl + P` | Ouvrir le panneau des rôles |
| `Ctrl + L` | Ouvrir le sélecteur de langues, hors modale |
| `Alt + E` | Ouvrir le menu des émotions du texte |
| `Ctrl + K` | Scinder le dialogue à la position du caret |

Dans la liste des projets récents, `↑`/`↓` parcourent les projets, `Entrée`
ouvre le projet, `Suppr` le retire de la liste et `Échap` ferme la liste.

Dans les paramètres du projet, l’option « Définir la couleur du texte à la
couleur de l’étiquette de personnage » applique la couleur de l’étiquette au
texte des lignes défilantes non karaoké.

## Outils et lecture

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

Sur une détection sélectionnée, `Ctrl + Espace` auditionne le signe avec une
courte plage audio autour de celui-ci. `Ctrl + Maj + Espace` ajoute un point de
sync au curseur de lecture.

## Bande rythmo

| Raccourci | Action |
|---|---|
| `Entrée` | Sélectionner successivement les lignes présentes au frame courant |
| `Maj + clic-glissé` (zone vide) | Ajouter les lignes du rectangle à la sélection existante |
| `↑` / `↓` (sélection active) | Déplacer la ou les lignes vers la piste précédente/suivante |
| `Maj + ←` / `Maj + →` | Parcourir toutes les lignes ; sans sélection, commencer par la plus proche de la tête de lecture |
| `Ctrl + Maj + ←` / `Ctrl + Maj + →` | Décaler la ou les lignes sélectionnées d’une frame vers la gauche/droite |
| `Échap` | Annuler la sélection actuelle de lignes |
| `I` / `O` | Fixer le début/la fin de la ligne sélectionnée |
| `Q` / `D` (maintenir) | Déplacer continuellement la timeline à gauche/droite |
| `T` | Modifier le texte de la ligne sélectionnée |
| `P` | Modifier le personnage de la ligne sélectionnée |
| `Suppr` | Supprimer la ou les lignes sélectionnées |
| `Ctrl + C` | Copier la ou les lignes sélectionnées |
| `Ctrl + X` | Couper la ou les lignes sélectionnées |
| `Ctrl + V` | Coller la ou les lignes au curseur de lecture, sur la piste sous la souris |

Le collage multiple conserve les écarts temporels et les écarts de piste entre
les lignes. Si la souris est hors de la bande rythmo, `Ctrl + V` utilise la piste
clavier active. Pendant l’édition d’une ligne ou d’un personnage, les flèches
déplacent le caret et ne modifient pas le volume.

Lorsqu’une détection est sélectionnée, `Maj + ←`/`Maj + →` déplacent son ancre
de synchronisation d’un grapheme. Appuyer sur `Maj` seul bascule l’affinité de
l’ancre.

## Export et proxy

| Raccourci | Action |
|---|---|
| `Tab` / `Maj + Tab` | Parcourir tous les réglages, formats, langues et boutons |
| `↑` / `↓` sur le menu des pages | Passer entre Vidéo, Sous-titres, Audio et Références |
| `←` / `→` sur le menu des pages | Passer entre les pages d’export |
| `↑` / `↓` sur un réglage | Modifier sa valeur |
| `Entrée` / `Espace` | Activer un format, une langue ou un bouton |
| `Échap` | Fermer l’export ou annuler l’opération en cours |

Les champs largeur et hauteur acceptent les chiffres, `Retour arrière` et
`Entrée` termine leur édition. Dans la création de proxy, `Tab` / `Maj + Tab`
parcourent la résolution, la qualité et le bouton de création. Les flèches
règlent la valeur focalisée, `Entrée` / `Espace` l’active et `Échap` ferme la
modale.

## Champs texte, panneaux et modales

| Raccourci | Action |
|---|---|
| `Ctrl + A` | Tout sélectionner |
| `Ctrl + C` / `Ctrl + X` / `Ctrl + V` | Copier/couper/coller le texte |
| `Ctrl + Z` | Annuler |
| `Ctrl + Maj + Z` | Rétablir |
| `←` / `→` | Déplacer le caret |
| `Maj + ←` / `Maj + →` | Étendre la sélection |
| `↑` / `↓` | Parcourir les suggestions ou les éléments de liste |
| `Retour arrière` / `Suppr` | Supprimer le caractère précédent/suivant |

Dans un panneau ouvert, `Ctrl + A`, `Ctrl + C`, `Ctrl + X`, `Ctrl + V` et
`Ctrl + Z` s’appliquent à l’édition ou à la sélection du panneau. Dans
l’Explorateur de fichiers, `Alt + ←`/`Alt + →` parcourent l’historique,
`Retour arrière` remonte au dossier parent et `Entrée` ouvre le fichier ou
valide l’action principale.

## Fenêtre secondaire

| Raccourci | Action |
|---|---|
| `Espace` | Lecture/pause |
| `Échap` | Fermer la fenêtre secondaire |

## Lecture vocale interne

| Raccourci | Action |
|---|---|
| `Ctrl + Maj + N` | Activer/désactiver la lecture vocale interne |
| `Ctrl` | Interrompre la lecture vocale en cours |
| `Maj` | Reprendre la lecture vocale interrompue |

Chaque changement de focus, sélection, activation, valeur et résultat d’action
est annoncé. Une ligne karaoké est identifiée après son numéro de piste. Pour
`Q`/`D`, le timecode est annoncé une seule fois au relâchement de la touche,
avec les heures, minutes, secondes et centièmes.

L’ouverture d’une liste annonce son premier élément disponible. Sa fermeture
annonce que la liste est réduite. La fermeture d’une modale est également
annoncée.

Sur Linux, la lecture vocale interne reste indisponible et le raccourci affiche
un message localisé.
