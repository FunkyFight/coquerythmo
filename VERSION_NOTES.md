# 4.2.0

- Lecture audio : les fichiers stéréo sont lus en stéréo ; les enregistrements restent traités en mono.
- Ajout d’un réglage de volume indépendant par piste dans le DAW, local à la station et non partagé.
- Correction du mix de prévisualisation et de l’export des enregistrements pour conserver le mono.
- Ajout de l'envoi des params de BR aux comédiens.

# 4.1.0

- Ajout d’un système de mise en place rapide par liens `coquerythmo://` pour configurer un projet et créer ou rejoindre un salon.
- Retrait de l’état « Salon créé » et du code du salon de l’interface ; le code apparaît désormais dans le titre de la fenêtre après l’état du salon.
- Les fichiers temporaires internes utilisent par défaut le dossier temporaire de Windows et peuvent être déplacés depuis les Paramètres ; ils ne sont plus créés dans le dossier d’installation.
- Les sauvegardes automatiques, transferts reçus, proxys vidéo et extractions de projets utilisent les dossiers de données ou temporaires de l’utilisateur.
- L’explorateur de fichiers personnalisé est supprimé : toutes les sélections de fichiers et de dossiers utilisent désormais l’explorateur Windows.
- La console affiche au démarrage un rappel en jaune gold et en gras ainsi que l’utilité des logs pour le diagnostic des bugs.

# 4.0.7

- Enregistrement en ligne : un comédien peut rejoindre un salon sans projet ouvert ou avec un projet différent.
- Ajout du transfert de projet demandé par le DA, avec réponse de chaque participant, expiration après 60 secondes et modale globale de suivi.
- Transfert du fichier `.coquerythmo` par morceaux de 192 Kio, vérification SHA-1, finalisation atomique, gestion des collisions et ajout aux projets récents.
- Prise en charge des projets locaux modifiés : sauvegarder et remplacer, remplacer sans sauvegarder ou refuser.
- Ajout des traductions française, anglaise et espagnole et des annonces d’accessibilité associées.
- Les écrans d’attente n’affichent plus de barre de progression tant qu’aucun transfert de fichier n’est en cours.
- Le remplacement nettoie les références FLAC orphelines et verrouille clairement le choix du comédien après réponse.
- Les résultats de transfert répétés après la clôture sont maintenant traités idempotemment.
- Les microphones incompatibles restent visibles dans la sélection, avec une raison courte en rouge, mais ne peuvent pas être choisis.
- Les prises et projets reçus utilisent désormais les dossiers temporaires et de données de l’utilisateur, jamais le dossier d’installation protégé.
- Le micro de chaque comédien est testé avant la prise et le DA voit immédiatement qui n’est pas prêt ; le lancement reste bloqué tant qu’un micro actif échoue.
- La préparation canonique de la timeline précède maintenant le compte à rebours, et les erreurs d’envoi audio ne sont plus silencieuses.




# À faire

- Poignées karaoké sur toute la hauteur de leur ligne.
- Exporter et importer les nodes.
