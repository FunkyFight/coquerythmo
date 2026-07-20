# Refactor V4 — checklist smoke test

Cette checklist fige les parcours à vérifier manuellement pendant le refactor.
Elle ne constitue pas une nouvelle spécification produit : chaque résultat
attendu est « identique à la version de référence ».

## Préparation

- [ ] Lancer l'application avec la configuration utilisateur existante.
- [ ] Vérifier que la fenêtre, son titre, sa taille initiale et son icône sont inchangés.
- [ ] Conserver un projet de test avec vidéo, audio, lignes, marqueurs, karaoké, dessins, personnages et comédiens.

## Parcours critiques

- [ ] Ouvrir un projet existant.
- [ ] Sauvegarder un projet existant, puis sauvegarder sous un nouveau chemin.
- [ ] Importer un projet natif, un projet Cappela et un fichier SRT.
- [ ] Exporter un projet et exporter une vidéo courte avec audio original et instrumental.
- [ ] Charger une vidéo, lire/pause, avancer/reculer d'une frame et utiliser la timeline.
- [ ] Activer/désactiver l'audio actif, régler le volume et vérifier le décalage audio.
- [ ] Créer, déplacer, redimensionner, éditer et supprimer une ligne.
- [ ] Vérifier undo, redo, dirty flag et sauvegarde après une mutation.
- [ ] Activer le karaoké, modifier les syllabes et vérifier l'affichage de la progression.
- [ ] Dessiner, effacer et transformer un ou plusieurs traits.
- [ ] Ouvrir, utiliser, fermer et annuler chaque modale existante.
- [ ] Se connecter à un salon, recevoir une commande distante et demander une synchronisation.
- [ ] Basculer entre Bande rythmo et Enregistrement ; vérifier que la vue reste identique et en lecture seule dans Enregistrement.
- [ ] Ouvrir l'écran secondaire, vérifier lecture/pause, Échap, redimensionnement et fermeture.

## Raccourcis à vérifier dans chaque contexte

- [ ] Échap dans une modale, en édition de texte, dans chaque espace et sur l'écran secondaire.
- [ ] Espace dans chaque espace et sur l'écran secondaire.
- [ ] Tab, Suppr et les flèches.
- [ ] Ctrl+K, Ctrl+S, Ctrl+N.
- [ ] Ctrl+A/C/X/V/Z et Ctrl+Maj+Z en édition de texte et dans l'éditeur.
- [ ] Répétition d'une touche et relâchement d'une touche.

## Compatibilité

- [ ] Les textes UI, valeurs par défaut, chemins initiaux et filtres de fichiers sont inchangés.
- [ ] Les onglets d'espace sont visibles, navigables au clavier et annoncent correctement leur sélection.
- [ ] Rejouer la checklist sur Windows ; exécuter les variantes de plateformes déjà supportées lorsque disponibles.
