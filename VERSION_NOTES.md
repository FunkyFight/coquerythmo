# v3.5.0

## Accessibilité clavier et lecture vocale

Cette version améliore l’utilisation de Coquerythmo sans souris, avec un
routage clavier commun, des contrôles focalisables et des annonces vocales
sémantiques.

### Navigation et modales

- Ajout d’une navigation `Tab`/`Maj + Tab` avec focus visible et retour au
  contrôle déclencheur après fermeture d’une modale.
- Les contrôles, listes, menus déroulants, sélecteurs et modales acceptent
  `Entrée`/`Espace`, les flèches, `Début`/`Fin`, `Page haut`/`Page bas` et
  `Échap` selon leur rôle.
- Le focus est enfermé dans la modale active et les contrôles désactivés sont
  ignorés.
- L’Explorateur de fichiers est utilisable au clavier, y compris la liste des
  fichiers, l’historique, la recherche, les suggestions et la confirmation de
  remplacement.
- Les réglages du projet et tous les paramètres de l’export sont parcourables
  et modifiables sans souris.

### Raccourcis ajoutés et complétés

- Import unifié : `Ctrl + Maj + I`.
- Restauration : `Ctrl + Maj + R`.
- Projets récents : `Ctrl + R`.
- Fermeture du projet : `Ctrl + Suppr`.
- Export : `Ctrl + M`.
- Avertissement Studio : `Ctrl + Alt + S`.
- Renommage d’un personnage : `Ctrl + Maj + P`.
- Paramètres du projet : `Ctrl + O`.
- Navigation image précédente/suivante : `Ctrl + ←` / `Ctrl + →`.
- Outils et marqueurs du pavé numérique `1` à `9`.
- Volume par pas de 5 % avec `Maj + ↑` / `Maj + ↓`, et muet avec
  `Maj + Numpad -`.
- Création, sélection, édition et déplacement de lignes avec `Insert`,
  `Entrée`, `I`, `O`, `Q`, `D`, `T`, `P` et `↑`/`↓`.
- Copie, coupe et collage d’une ligne avec `Ctrl + C`, `Ctrl + X` et
  `Ctrl + V`. Le collage utilise la piste sous le curseur de souris, ou la
  piste clavier active si le curseur est hors de la bande rythmo.

### Export

- Les pages Vidéo, Sous-titres, Audio et Références sont navigables avec les
  flèches.
- `Tab`/`Maj + Tab` parcourent les pages, formats, réglages vidéo, langues,
  options audio et boutons d’action.
- Les valeurs numériques, formats et sélections de langues peuvent être
  modifiés et activés au clavier.

### Lecture vocale interne

- `Ctrl + Maj + N` active ou désactive la lecture vocale interne.
- Les annonces portent sur le focus, les sélections, les valeurs, les actions,
  les réussites, les erreurs et l’ouverture/la fermeture des modales.
- Les annonces rapides interrompent immédiatement la voix précédente afin de
  décrire l’action la plus récente.
- Les symboles décoratifs et caractères spéciaux ne sont pas lus comme des
  mots ; la saisie n’est pas annoncée caractère par caractère.
- Les champs sensibles n’exposent jamais leur valeur.
- Le déplacement continu avec `Q`/`D` annonce le timecode une seule fois au
  relâchement, en heures, minutes, secondes et centièmes.
- La synthèse utilise le backend système disponible sur Windows et macOS. Sur
  Linux, le raccourci reste désactivé et affiche un message localisé.

### Documentation

- `RACCOURCIS_CLAVIER.md` recense les raccourcis, les contextes et les parcours
  clavier des modales.
