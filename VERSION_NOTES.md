# 3.6.0

## Lecture audio

- Correction du décalage positif de l'audio instrumental dans l'interface et à l'export : la lecture et les fichiers exportés respectent désormais le silence initial demandé avant de démarrer l'instrumental, y compris lorsque la cadence d'export diffère de celle de la source.

## Export audio et annonceur karaoké

- Ajout de variantes audio sélectionnables séparément pour chaque langue dans le centre d’export :
  - audio original (`O`) ;
  - audio instrumental (`I`) ;
  - audio original avec annonceur (`O+`) ;
  - audio instrumental avec annonceur (`I+`).
- Sous Windows, les variantes avec annonceur utilisent la voix SAPI Windows à 150 %.
- L’annonce est calée pour se terminer une seconde avant le début du chant.
- Les changements sont évalués indépendamment sur chaque piste karaoké ; les lignes normales sont ignorées.
- Les changements simultanés sont regroupés dans une seule annonce, par exemple « Alice, Bob et Chloé ».
- Les variantes sont disponibles pour les exports vidéo MP4 et les exports audio seuls.
- Les contrôles d’export disposent d’un espacement uniforme et suivent le même ordre en affichage et au clavier : `O`, `I`, `O+`, `I+`.

## Réparation du karaoké
- les lignes karaoké n'apparaissent qu'à partir du compte à rebours de leur propre balle ;
- les lignes serrées ne sont plus dessinées les unes sur les autres sur une même rangée ;
- l'onglet Enregistrement et les commandes du mini-DAW sont bloqués lorsque DEV_MODE est désactivé ;
- aucune barre d'onglets vide n'est conservée en production.
- centralise la sélection de la ligne karaoké visible par rangée dans la scène de rendu ;
- donne toujours priorité à la ligne actuellement active sur les aperçus futurs ;
- lorsqu’aucune ligne n’est active, conserve l’aperçu futur le plus proche au lieu du plus lointain ;
- applique la même règle à l’éditeur interactif, au rendu CPU, au rendu GPU et aux exports via la scène partagée ;
- ajoute une régression avec des lignes de 12 frames à 24 FPS, soit 0,5 seconde.


## Syllabification par langue de rythmo - broken
- ajoute un réglage Français / Anglais propre à chaque langue du projet dans le panneau Langues ;
- utilise ce réglage pour le découpage des syllabes, les poignées d’édition, la découpe de dialogue, le karaoké et les exports ;
- transmet la même valeur à la scène de rendu, au rendu CPU et au rendu GPU ;
- invalide les timings syllabiques manuels uniquement pour la langue dont la règle change ;
- ajoute une navigation clavier déterministe, un rôle/une valeur accessibles et des annonces de changement ;

## Détections et points de synchronisation

- les signes de détection sont rattachés à une piste et à un temps média absolu, indépendamment des lignes de dialogue ;
- la palette expose neuf signes professionnels : labiale, semi-labiale, bouche ouverte, bouche fermée, dents visibles, TH, respiration, neutre et réaction ;
- `Alt+D` ouvre la palette tandis que `D` conserve sa navigation horizontale existante ;
- seuls les neuf SVG utilisés sont conservés sous `src/icons/detection` ;
- les signes sont sélectionnables, déplaçables, navigables, ajustables, supprimables et pris en charge par l’historique ;
- un clic simple sur un signe ouvre une grande fiche avec une bouche Rhubarb, son nom, sa description et les sons correspondants ;
- un glisser démarre directement depuis un signe existant ou depuis un nouveau point de synchronisation, avec un seuil de quatre pixels pour distinguer clic et déplacement ;
- les signes sont ancrés en bas de leur ligne et leur trait vertical reste aligné sur leur axe ;
- `Ctrl+Espace` écoute les deux secondes disponibles avant et après le signe sélectionné avec un bip exactement au repère ;
- les lignes de dialogue affichent des points de synchronisation par caractère, persistants et déplaçables ;
- les annonces AccessKit restent limitées à l’opération visuelle : ajout, déplacement ou suppression du signe ou du point ;
- les détections et points de synchronisation participent à la persistance, à la conversion de cadence et à la collaboration.
