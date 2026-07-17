# v3.5.4

## Accessibilité Windows et navigation clavier

Cette version renforce l’accessibilité de Coquerythmo avec une intégration
AccessKit sous Windows, des annonces vocales plus systématiques et une
navigation clavier étendue dans les listes, les modales et l’Explorateur de
fichiers.

### Lecture vocale et annonces

- Utilisation de `accesskit_winit` sous Windows, compatible avec le système
  d’accessibilité déjà en place.
- Les raccourcis nommés annoncent uniquement leur action, sans préfixe
  « raccourci », sans combinaison de touches et sans fallback sur la touche.
- L’ouverture d’une liste annonce son premier élément disponible.
- La fermeture d’une liste annonce qu’elle est « réduite ».
- La fermeture d’une modale annonce qu’elle est « fermée ».
- Le chargement, la réussite ou l’échec du chargement d’un projet sont
  annoncés.
- `Ctrl` interrompt la lecture vocale et `Maj` la reprend.
- Les annonces d’actions utilisant `Ctrl` sont prioritaires : NVDA peut
  interrompre la voix courante sans masquer l’action exécutée.
- Tous les toasts sont vocalisés sur le canal prioritaire.
- Les exportations et créations de proxy sont exposées au lecteur d’écran
  comme des barres de progression. Leur opération et leur pourcentage sont
  rappelés toutes les minutes sur le canal prioritaire.
- Le démarrage et l’annulation d’un export ou d’un proxy sont annoncés en
  priorité ; `Échap` permet d’annuler l’opération en cours.
- Les réglages du projet et les paramètres annoncent correctement le focus,
  les contrôles et les actions.

### Explorateur de fichiers et projets

- Avec la lecture vocale active sous Windows, les sélecteurs de fichiers
  utilisent l’Explorateur Windows natif.
- Sans lecture vocale, l’Explorateur de fichiers personnalisé reste utilisé.
- Les actions de sauvegarde et de navigation dans l’Explorateur personnalisé
  sont accessibles au clavier.
- Les projets `.coquerythmo` peuvent être ouverts avec Coquerythmo depuis
  Windows, notamment via « Ouvrir avec Coquerythmo ».

### Raccourcis de lignes

- `Ctrl + Numpad 1` à `Ctrl + Numpad 4` insèrent une ligne sur la piste
  correspondante. Le pavé numérique est requis.
- Le raccourci `Insert` est retiré.
- `Maj + Flèche gauche` et `Maj + Flèche droite` parcourent toutes les lignes,
  en commençant sans sélection par la ligne la plus proche de la tête de
  lecture, puis annoncent le personnage, le dialogue, le numéro de piste et
  l’état karaoké si présent.
- La bascule du karaoké annonce uniquement « activé » ou « désactivé ».
- `Échap` annule la sélection actuelle de lignes et `Espace` contrôle la
  lecture/pause.
- `Ctrl + Maj + P` ouvre la création d’un proxy. Sa résolution, sa qualité et
  son bouton de création sont entièrement navigables au focus avec `Tab`, les
  flèches, `Entrée`, `Espace` et `Échap`.

### Documentation

- Cette version complète les parcours clavier et les annonces vocales décrits
  dans `RACCOURCIS_CLAVIER.md`.
