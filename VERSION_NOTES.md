# 3.6.1
- Correction du déchirement temporel
- Correction affichage étiquettes karaoké
- Correction affichage tardif étiquette quand ligne juste avant despawn
- Possibilité de déplacer la barre verticale de lecture


# To do :
Poignées karaoké font toute la hauteur de leur ligne
Exporter / Importer les nodes

## Text Emotions

Résumé
Ajoute les Émotions du texte aux lignes de dialogue de la bande rythmo, avec rendu animé partagé entre l’interface, le renderer GPU et le fallback CPU.

Effets inclus :

Pendule
Balancement
YAY!!!
Bounce
Glissade
Oscillation
Vague
Tremblement
Wiggle
Interaction
Alt+E ouvre la palette sur la ligne sélectionnée ou la sélection de texte active.
Navigation complète au clavier : flèches haut/bas, Début/Fin, Entrée et Échap.
La première option est toujours Retirer l’émotion.
Le clic droit sur une ligne éligible expose Émotion du texte.
Une ligne sélectionnée applique l’effet à toute la réplique.
En édition, une sélection non vide limite l’effet à cette plage.
Les bornes sont exprimées en graphèmes étendus pour ne pas découper les accents, ligatures ou emoji composés.
L’option est absente pour les ambiances et les lignes karaoké.

Accessibilité
La palette est une surface modale qui capture ses propres commandes clavier.
Ouverture, focus, changement de sélection, validation, erreur et fermeture publient des événements sémantiques vers AccessKit/NVDA.
Le lint de la ligne reste inclus dans sa description accessible.

Lint
Toute ligne contenant au moins une émotion reçoit l’avertissement :

N'utilisez pas d'émotions du texte dans un milieu professionnel qui ne l'autorise pas !

# Bugs
Dans l'export : une fois ligne karaokée finie, elle se tp sur la ligne verticale au lieu de disparaître
Impossible de mettre une couleur à une étiquette, ça despawn et annule le clic
Caret mal placé encore dans les dialogues.
Points de synchro ne correspondent pas à là où j'ai cliqué