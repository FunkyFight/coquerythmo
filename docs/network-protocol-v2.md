# Protocole réseau 2

Le client et le serveur doivent être mis à jour ensemble. La négociation Socket.IO
et les réponses d'admission portent `protocol_version: 2`. Un pair incompatible
est refusé explicitement. Le serveur utilise exclusivement WebSocket.

## Cycle de vie d'un projet

1. Le DA prépare les métadonnées de son archive sauvegardée dans un worker.
2. `project_begin` réserve le stockage du salon et renvoie le premier bloc manquant.
3. `project_write` envoie des blocs de 192 Kio, avec index et accusé de réception.
   Une réponse perdue entraîne la répétition du même bloc. Le serveur compare les
   octets déjà écrits avant d'accepter un doublon.
4. `project_commit` vérifie la taille et le SHA-1, puis synchronise le fichier sur
   disque. L'archive devient disponible uniquement après cette étape.
5. Chaque participant accepte, télécharge avec `project_read`, vérifie son fichier
   local, puis importe le projet. Ses réponses et progrès sont indépendants de
   ceux des autres participants. Un téléchargement continue pendant que le DA
   travaille, et les nouveaux arrivants peuvent utiliser le cache sans le DA.
6. Quand le dernier socket quitte le salon, celui-ci est retiré immédiatement de
   l'annuaire. La fermeture des fichiers et leur suppression sont attendues après
   les opérations disque en cours. Les réservations de stockage sont libérées.

Le cache appartient au salon, pas au DA. Une coupure du DA ne supprime donc pas le
projet si d'autres personnes sont encore connectées. Un nouvel envoi identique
réutilise l'archive et son identifiant, pour préserver les téléchargements actifs.
Une nouvelle version remplace l'archive publiée après vérification. Les fichiers
ne sont pas exposés par une URL HTTP publique.

Les fichiers partiels expirent après dix minutes d'inactivité. Chaque processus
possède un répertoire marqué. Au démarrage, le serveur nettoie les répertoires
marqués des processus arrêtés et préserve ceux des autres instances actives.

## Requêtes et reconnexion

`protocol_request` contient `{ id, method, body }`. La réponse `protocol_reply`
reprend l'identifiant et contient soit `{ ok: true, body }`, soit
`{ ok: false, error, retryable }`. Une écriture WebSocket réussie ne termine pas
une opération : le client attend l'accusé de réception du serveur.

Les identifiants combinent un nonce de 128 bits et un compteur. Le client retente
la même requête après cinq secondes sans réponse, pendant au plus cinq minutes.
Les erreurs de saturation ou d'admission temporaire sont retentées. L'arrêt
volontaire de la session annule les workers et les téléchargements.

Le client reconnecte entre 500 ms et 10 s. Il mémorise le salon dès le callback
d'admission, avant le traitement de l'interface, et le rejoint avec son jeton de
session. Le rôle du DA et les habilitations Co-DA sont conservés ; un acteur
n'est jamais promu automatiquement lorsque le DA disparaît.

L'ouverture et la fermeture de la connexion s'effectuent hors du thread
d'interface, y compris si le serveur ne répond pas. Les sauvegardes locales
conservent l'identité de l'archive publiée pour la reconnexion.

## Réplication de la bande rythmo

Le DA est l'auteur de l'état partagé. Le serveur conserve un numéro de révision
et refuse toute écriture d'un acteur. Un commit contient une révision de base,
un identifiant d'opération et les entités modifiées ou supprimées. Les répétitions
d'un commit ne l'appliquent pas une seconde fois.

Le document comprend toutes les langues, leur ordre et la langue active, les
répliques complètes et leur ordre, les marqueurs, personnages, doubleurs et icônes
intégrées, dessins, détections, synchronisations du texte, automatisations et
réglages du projet. Sélection, trait en cours, affichage des pistes vides, famille
de police, lecture, position et choix de piste audio sont également partagés.
Undo/redo et imports passent par le même état, sans dépendre d'une liste de
gestes d'interface à retransmettre.

Les chemins de médias et d'icônes restent locaux au destinataire. Les médias et
polices intégrés proviennent de l'archive téléchargée. Ajouter un nouveau média
ou une police absente chez les destinataires nécessite de publier l'archive qui
le contient ; le choix de famille de police seul ne transfère pas un fichier de
police.

Le client regroupe les modifications rapprochées dans une boîte bornée à la
dernière version. La sérialisation, le calcul des différences et les échanges
s'effectuent dans un worker. Une modification de texte n'envoie que la réplique
concernée. Les lecteurs récupèrent les changements manqués dans le journal, ou
un instantané immuable découpé et vérifié si le journal ne suffit plus. Un
contrôle périodique rattrape aussi une notification `state_changed` perdue.
L'application du document attend la fin de l'import de l'archive, afin que les
modifications récentes ne soient pas remplacées par sa version sauvegardée.
Un lecteur lent conserve son propre instantané ; les nouveaux lecteurs accèdent
à la révision récente. Préparer ou annuler une nouvelle archive laisse l'archive
publiée accessible jusqu'au commit de sa remplaçante.

La lecture utilise des points de référence horodatés, renouvelés au moins toutes
les 500 ms pendant la lecture, avec compensation du temps de transport et du
téléchargement de l'instantané. Le client corrige une dérive supérieure à deux
images. Cette compensation ne remplace pas une mesure de latence sur le réseau
réel de production.

## Commandes d'enregistrement et de salon

La méthode RPC `event` enveloppe les commandes de gestion du salon, les commandes
d'enregistrement et les blocs de prises audio. Leurs règles d'autorisation et la
chaîne d'intégrité des transactions restent appliquées côté serveur. Les
accusés de réception sont dédupliqués par session, y compris après remplacement
du socket. Le worker audio ne place qu'un bloc à la fois dans la file commune,
pour laisser passer les commandes interactives, et annonce la réussite après
l'acceptation de `audio_end`.

Un ACK de relais signifie que le serveur a traité la commande ; il ne signifie
pas que tous les destinataires l'ont sauvegardée. Les prises audio et les gros
instantanés d'enregistrement restent des relais validés, distincts du cache de
projet. Une coupure qui détruit un relais partiel nécessite sa republication ;
elle renvoie une erreur plutôt qu'une réussite silencieuse. La reconnexion
demande l'état d'enregistrement et ses assets au DA disponible.

## Bornes par défaut

| Ressource | Limite |
| --- | --- |
| Archive de projet | 64 Gio |
| Réservations de stockage du processus | 128 Gio |
| Archives par salon | Une publiée et un remplacement en cours |
| Bloc de projet / état | 192 Kio avant base64 |
| Trame Socket.IO | 1 Mio |
| Requête RPC | 512 Kio |
| Requêtes en cours par socket | 8 |
| Document rythmo | 64 Mio |
| Budget partagé d'état / instantanés / commits préparés | 256 Mio |
| Journal par salon | 256 révisions et 8 Mio maximum |
| Reçus de commits / commandes par salon | 512 / 2048 |

Les salons et leur état sont temporaires : un redémarrage du serveur les termine.
Le stockage local du projet reste la source durable. Un salon devenu vide n'est
pas conservé dans l'attente d'une éventuelle reconnexion.

## Vérification

Depuis `server/`, `npm test` couvre les quotas, blocs répétés ou corrompus,
nettoyages, rôles, commits atomiques, journaux et échanges WebSocket réels.

Depuis la racine, `cargo test --offline` couvre le codec complet, l'annulation,
la boîte de réplication et les régressions de l'application. Avec Node et les
dépendances serveur installés, lancer également :

```text
cargo test --offline --lib network::integration_tests -- --ignored --test-threads=1
```

Ce test démarre un vrai serveur local, transfère une archive de plusieurs Mio,
réplique une bande dépassant une trame, perd des ACK d'upload et de téléchargement,
coupe les sockets, vérifie le fichier reçu, le rôle restauré, la déduplication
d'une commande d'enregistrement et les modifications après reconnexion.
