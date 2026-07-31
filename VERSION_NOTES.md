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
