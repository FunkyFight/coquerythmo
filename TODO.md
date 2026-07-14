# Idées :
Corriger les lignes qui poppent en mode gpu, cpu et UI (à cause de l'étiquette nom personnage devant)

Traduction auto des lignes
    => Gestion simplifiée et switch easy des différentes langues (CTRL + L)

Pouvoir dessiner sur la bande rythmo.
Tous les trucs de Voxdub qui comparent intégrés
F9 qui, selon le contexte (intelligent) affiche à gauche tous les raccourcis claviers possibles (catégorisés selon le panel)
Fichier .coquerythmo qui contient : la BR, la vidéo, les audios, les configurations, la police aussi, tout.
    => Passer le .json tout seul en legacy
Export to anything
    Pré-format : 16:9 (YouTube), 9:16 (Shorts / TikTok), Same as Source
    Qualité : 720p, 1080p, 1440p, 4k, 8k, custom
    Exports avec quels audios
    Exports avec quelles langues
Exports possibles :
    Vidéo
    Sous-titres : JSON, SRT, ASS, DETX
    Audio : mp3, wav, bwf stems
    Référence croisée : CSV, PDF
    Grille de croisillées (Grille de présence, boucles) https://encrypted-tbn0.gstatic.com/images?q=tbn:ANd9GcQkSPDwVRmkXZFyXFbooHqTWDFPm-QIGwJOJB18RNKsw4rSKMhTcL8CfMfs&s=10
Compte à rebours / Pre-roll (maybe deux choses différentes)
DAW basique.
Agrandir le texte pour le nom des persos
Afficher les fichiers qui ont le même nom quand on tape dans "Nom" dans l'explorateur de fichiers
Si pas sauvegarde et qu'on ferme : demander si on veut vraiment partir ou si on sauvegarde avant
Si Proxy : appliqué automatiquement à chaque load

Taille lignes karaoké dans l'export : le 200% devrait être le nouveau 100%
Taille Ligne BR dans l'export le 50% devrait être le nouveau 100%

Bug : Les handles pour les syllabes ne sont pas bien mises, elles coupent mal. Mais dès que j'en bouge une, elles coupent bien de nouveau


# Suggestions

Des raccourcis pour marquer le début et la fin d'une réplique ( comme un in et out dans un logiciel de montage ) et déplacer une réplique verticalement mais pas horizontalement ( un shift + click ) pour pas casser la synchro.

Une liste des répliques à gauche du player vidéo qui permette de se déplacer dans les différentes répliques ou même de les organiser à notre guise pour nous y retrouver par rôle par exemple ( comme dans un logiciel de montage) .

Un menu de rôle qui permette de paramétrer la couleur mais aussi l'emplacement sur la ligne du personnage ( j'aime beaucoup le menu de DubInstante https://dubinstante.vercel.app/ mais juste la couleur, la ligne par défaut et la taille je pense feraient gagner un paquet de temps.).

Et enfin si cliquer sur la timeline permet de se déplacer à l'endroit du click ca serait paas de refus mais j'ai conscience d'en demander beaucoup 😆

Un truc que j'avais vu sur le beta test de Voxdub V3 et que je trouvai bien c'est qu'on peut régler la vitesse de lecture de la rythmo en "dezoomant" de cette dernière. 

## Workflow chez les pros
1) Importer un .SRT
2) Outil Organisation Automatique
    Tu crées tes rôles et tu définis un comportement pour les lignes avec ces rôles (SI role EST Juliette ALORS mettre ligne sur track 2)
3) Panel pour avoir un aperçu de toutes les lignes où tu peux redéfinir les rôles des lignes que tu sélectionnes
4) Grâce au comportement automatique que tu as défini, les lignes s'organisent d'elle-mêmes ?