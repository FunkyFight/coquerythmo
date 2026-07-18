# Onglet
## Introduction

Ce document décrit le design et les fonctionnalités d'une toute nouvelle branche de l'application qui permettrait de s'enregistrer hors ligne ou en ligne sur une bande rythmo de Coquerythmo.
Ceci est à titre de référence, il faudra think et build upon it pour rendre le tout le plus complet possible.

Tout doit être SOLID, extensible, modifiable, DRY.

Le mode studio alpha est retiré.

## Fonctionnalités
### HUUID : Hyper-Unique User Id
Chaque projet sauvegardé se voit attribuer un HUUID.
Il est composé de :
Coquerythmo-[Version Coquerythmo]-[Date de la sauvegarde]-[UUIDv4 Classique]

À chaque sauvegarde, le HUUID change.

### Onglets 
En dessous de la top bar, deux onglets cliquables :
Bande Rythmo et Enregistrement

#### Bande Rythmo
L'app dans son état actuel avec ses raccourcis, toute la suite pour créer une bande rythmo

#### Enregistrement
Le nouveau mode qu'on cherche à créer

## Onglet d'enregistrement
Tout change :
La top bar à ses options dédiées à l'enregistrement (seulement le menu Projet qui ne change pas). Pour l'instant, pas de bande rythmo, juste deux choix :

1) S'enregistrer en solo
On ne se connecte pas, on a l'interface en solo tout simplement.

2) S'enregistrer en ligne
On devra se connecter à un serveur coquerythmo, pour rejoindre en tant que comédien OU créer une session en tant que Directeur Artistique

Petit ajustement, on peut pas du tout rejoindre la room si on a pas le même projet, peu importe l'onglet.

### Enregistrement en solo
Un mini DAW s'ouvre. La bande rythmo est en mode lecture seule (on créé ce mode). Elle est beaucoup plus petite et prend largement moins de place que d'habitude. On a la vidéo. Et on a le principal : la timeline.

Dans la timeline, on a basiquement plein d'outils dans un menu d'outil à gauche de celle-ci : sélection, couper un clip audio...
Évidemment en sélection on peut cliquer, sélectionner, déplacer, supprimer...
À droite, un panel avec la liste des audios du projet. 
À gauche de chaque piste, il y a un bouton pour mute, pour solo, ou pour s'enregistrer dessus

#### Enregistrement
Quand un enregistrement se lance : toute l'UI disparaît (top bar incluse) pour laisser le plus de place aux éléments suivants :
La vidéo en grand format, la bande rythmo en lecture seule.
L'enregistrement commence à l'endroit où se trouvait la barre de lecture de la timeline.

Au dessus de la bande rythmo, en petit, on voit : la wave de la vidéo puis en dessous, la wave de la voix enregistrée

Il y a un compte à rebours de trois secondes avant de commencer. Le micro est enregistré automatiquement au début.

Pour finir l'enregistrement, il suffit d'appuyer sur ESCAPE pour retourner à l'écran précédent. L'audio est automatiquement ajouté et placé correctement.

## Enregistrement en ligne
Quand on sélectionne l'enregistrement en ligne, si on est pas connecté à un serveur, ça nous ouvre la server list.

Une fois connecté. Si on a créé la room : on est DA. Sinon on est comédien.

### Vue en temps que DA
Le DA a accès à l'écran timeline. Il dispose des mêmes fonctionnalités que le mode hors ligne. Toute modification se répercute directement chez le comédien.
La lecture de la bande rythmo est synchronisée et décidée par le DA uniquement. Le scrub aussi. 