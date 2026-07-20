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

#### Transactions
Une liste des transactions intelligentes et hyper optimisé O(log(n)) est constamment gardé, enregistré dans le projet, qui contient toutes les actions faites de manière à ce qu'on puisse reconstruire très facilement l'état d'une session (toutes les coupes, tous les placements, etc) en rejouant toutes les transactions.

Avant de rebuild : on vérifie que l'intégrité de la liste est complète.
La liste des transactions possède un curseur pour savoir ce qu'on doit exécuter ou non pour éviter de tout rebuild.

#### Enregistrement
Le nouveau mode qu'on cherche à créer. Tous les enregistrements sont en .flac.

#### Envoi d'audios
Il va falloir trouver un moyen d'envoyer des audios parfois lourds via connection websocket.

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
Le DA a accès à l'écran timeline. Il dispose des mêmes fonctionnalités que le mode hors ligne. Toute modification se répercute directement chez le comédien en envoyant les transactions à celui-ci à chaque fois.
La lecture de la bande rythmo est synchronisée et décidée par le DA uniquement. Le scrub aussi. Ce qui lui permet très facilement de montrer des choses au comédien.

Un nouveau panel pour gérer les utilisateurs apparaît en haut à droite de la vidéo (ancré à la limite du viewport fenêtre). On peut promouvoir en tant que Co-DA, mute/démute un comédien pour qu'il ne soit pas enregistré, le kick, ou le ban-ip (l'ip n'est jamais exposé au DA, c'est le serveur qui s'occupe de ban-ip)

Quand il lance un enregistrement ou qu'il lance la lecture vidéo, avant que ça se fasse, pour être sûr que tout le monde a le même état de timeline :
1) Les fichiers audios que les gens n'ont pas sont envoyés (au cas où des retardataires arrivent entre temps)
2) La liste complète des transactions est envoyée à tous
3) Tout le monde reconstruit à l'aide des transactions la timeline actuelle.

Le processus doit être hyper rapide et optimisé.

À la fin de l'enregistrement, le DA reçois les audios de tous dans son panel de liste des clips.
Les clips sont placés automatiquement dans le track qui a eu son bouton de record cliqué.

#### Co-DA
Un Co-DA a aussi la vue timeline. Il ne peut pas faire de modifications, celle-ci est en lecture seule.
Sa vue timeline est update à chaque modification du DA évidemment puisqu'il reçoit les transactions comme tout le monde. S'il souhaite faire une modification, le DA doit lui accorder le contrôle. La timeline n'est plus en lecture seule pour lui mais pour le DA si. Le DA peut retirer le contrôle à tout moment.

### vue en temps que comédien
Le comédien a seulement la vue enregistrement comme décrite ci-dessus. À la fin d'une session, le comédien possède donc dans son propre fichier l'entièreté de la timeline, des transactions et peut réexporter techniquement les pistes de son côté si il veut.