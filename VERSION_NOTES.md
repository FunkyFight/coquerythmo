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
