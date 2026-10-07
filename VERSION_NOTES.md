# Comic Dubs Studio

Le mode Comic Dubs devient un vrai petit studio de doublage de BD : on y
dessine les bulles, on place les plans de caméra case par case, on anime, on
enregistre et on mixe voix, bruitages et musique, et l’aperçu montre
exactement la vidéo exportée.

## Plans caméra

- Les cadrages sont désormais des **plans** posés sur la page, autant qu’on
  veut : outil « Plan caméra » dans la barre d’outils (ou « Tracer un plan »
  dans les réglages de la page), puis glisser sur la page. Le cadre suit
  toujours le format de la vidéo exportée ; un simple clic crée un plan
  standard.
- Les plans sont **toujours visibles** sur la page, numérotés (« PLAN 1 »,
  « PLAN 2 »…). On les déplace en glissant leur étiquette ou leur bord, on les
  redimensionne par les coins ; le plan sélectionné assombrit ce qu’il laisse
  hors champ. Le bouton « Plans » de la barre d’outils les masque.
- La vidéo enchaîne les plans dans leur ordre : la caméra rejoint le plan
  (mouvement fluide réglable ou coupe franche), marque une pause si besoin,
  puis ses bulles sont lues. Chaque bulle est montrée par le plus petit plan
  qui la contient (ou un plan choisi à la main) ; un plan sans bulle est tenu
  seul, comme un plan d’ensemble. « Page entière » crée un plan d’ensemble.
- Chaque bulle affiche le plan qui la montre (« P2 ») ; une bulle lue pendant
  un plan qui ne la cadre pas est signalée en rouge sur la page, dans
  l’inspecteur et sur la timeline, avec un bouton « Nouveau plan cadré sur
  cette bulle ».
- La timeline a une piste « Plans » ; cliquer un plan le sélectionne.

## Espace de travail

- Barre d’outils au-dessus de la planche avec de vraies icônes et des
  infobulles : Sélection, Bulle ronde, Bulle rectangulaire, Bulle de cri,
  Bulle de pensée, Cartouche de narration, Forme libre et Plan caméra. Après
  avoir tracé une bulle, on revient à la Sélection et on écrit directement son
  texte.
- Zoom sur la planche : `Ctrl` + molette autour du pointeur, molette ou clic
  molette pour se déplacer, bouton du zoom (« 100 % ») pour tout revoir.
- Panneau de gauche : miniatures des planches (monter, descendre, supprimer)
  et bibliothèque de sons (écouter, supprimer, glisser sur une bulle).
- L’inspecteur suit la sélection : **bulle** (Texte, Apparence, Animation,
  Son, Caméra, Organisation), **plan** (cadrage, arrivée, pause, bulles
  montrées, ordre, aperçu) ou, sans sélection, **Page** et **Projet**. Les
  sections se replient d’un clic.
- Réglages avec des curseurs (glisser, flèches du clavier ; un glissé = une
  seule étape d’annulation), des listes déroulantes qui montrent tous les
  choix, des interrupteurs et des boutons segmentés. Plus aucun symbole
  typographique en guise de bouton.
- Texte des bulles modifiable directement, avec un vrai curseur (flèches,
  `Début`, `Fin`, `Suppr`).
- Enregistrement à la chaîne : après chaque prise au micro, la bulle suivante
  sans voix est sélectionnée, prête à enregistrer.
- Une ligne d’aide contextuelle au-dessus de la timeline explique l’outil en
  cours.

## Effets

- Apparition des bulles (fondu, pop, zoom, glissements, chute avec rebond),
  bulle entière cachée avant son tour, machine à écrire ou mot par mot,
  emphase pendant la réplique (tremblement, pulsation, flottement,
  sautillement), effets d’écran (secousse, flash, impact), disparition après
  la réplique, sept préréglages de style, ombre portée, contour du texte.
- Transitions de page (fondu au noir, fondu enchaîné, glissements, zoom,
  flash), pause avant la première bulle, mouvement lent de caméra.
- Musique de fond en boucle, baissée automatiquement sous les voix, avec fondu
  de fin ; volume et délai de chaque voix, bruitage à l’apparition.

## Timeline et aperçu

- Pistes Pages, Plans, Bulles, Sons et Musique. Glisser sur la règle affiche
  l’image exacte du rendu ; cliquer un bloc sélectionne la bulle ou le plan
  (et change de page si besoin) ; bouton Lire / Arrêter.
- La lecture suit cette timeline : musique, voix et bruitages sont joués
  ensemble, et la bulle suivante n’arrive jamais avant la fin de la voix.

## Export

- Le rendu vidéo est recalculé image par image : plans caméra, transitions,
  animations, contours anti-crénelés, ombres, texte en italique et contour du
  texte sont identiques à l’aperçu. La musique de fond, les bruitages et le
  volume de chaque voix sont mixés dans la vidéo.
- Le rendu utilise tous les cœurs du processeur et les images identiques ne
  sont calculées qu’une fois.
- Le texte des bulles n’est plus légèrement écrasé verticalement à l’export.
- Nouveaux exports « Sous-titres des bulles (SRT) » et « Script de traduction
  (TXT) », et import d’un script : une ligne par bulle dans l’ordre de
  lecture, `# Page N` pour se recaler, `-` pour une bulle sans texte.

## Clavier et accessibilité

- Copier, couper, coller, dupliquer et déplacer les bulles au clavier, bulle
  précédente/suivante avec lecture de son texte, `Suppr` retire la bulle ou le
  plan sélectionné. Voir [`RACCOURCIS_CLAVIER.md`](RACCOURCIS_CLAVIER.md).
- Chaque outil, réglage, plan et bloc de timeline est un contrôle nommé pour
  les lecteurs d’écran ; les curseurs se règlent aux flèches et les listes
  s’ouvrent avec `Entrée`.

## Bande rythmo

- Le texte des répliques occupe maintenant toute la hauteur de la ligne : au
  survol, la zone illuminée ne dépasse plus au-dessus et en dessous du texte.
  Le changement s’applique aussi à l’export vidéo.
- La piste survolée n’est plus éclairée en entier : seule la réplique sous le
  pointeur est mise en évidence.

# 5.3.0

## Points de synchronisation

- Quand le pointeur reste un instant (1,5 s) sur l’endroit où un clic crée un point de synchronisation, une infobulle « Créer le point de synchronisation » l’indique.
- Un clic droit sur un point de synchronisation ouvre un menu contextuel : « Inclure le caractère courant » ou « Exclure le caractère courant » (comme `Maj`), et « Supprimer » (comme `Suppr`). Le menu se pilote aussi au clavier (flèches, `Entrée`, `Échap`).

## Formatage du texte

- Une barre de formatage apparaît au-dessus d’une ligne non karaoké dès qu’une partie de son texte est sélectionnée pendant l’édition. Elle met la sélection en gras, en italique, la souligne ou la barre. Un bouton allumé indique que tout le texte sélectionné porte déjà ce style ; un nouveau clic le retire. Le texte formaté garde l’étirement entre les points de synchronisation.
- Raccourcis : `Ctrl + B` (gras) et `Ctrl + U` (souligné) sur la sélection ou la ligne sélectionnée, et `Ctrl + I` (italique) pendant l’édition du texte. Hors édition, `Ctrl + I` ouvre toujours le panneau des lignes.
- Le formatage s’affiche dans la bande et dans l’export vidéo, suit le texte quand on le modifie, s’annule avec `Ctrl + Z`, est enregistré dans le projet et partagé en collaboration. Les exports SRT et ASS le conservent (balises `<b>`, `<i>`, `<u>`, `<s>` et `{\b1}`, `{\i1}`, `{\u1}`, `{\s1}`).
- Les lignes karaoké ne sont pas concernées.

## Style de la bande

- Nouvelle fenêtre « Style de la bande » (icône pinceau dans la barre du haut, à côté des paramètres du projet, ou `Ctrl + Maj + O`) : couleur du fond de la bande, couleur et épaisseur de la barre de lecture. Chaque réglage s’applique en direct sur la bande, qui reste visible à côté de la fenêtre ; « Annuler » ou `Échap` remet le style d’origine. Un clic sur la pastille de couleur ouvre un sélecteur (teinte, saturation, luminosité) ; la couleur se saisit aussi en code `#RRGGBB`.
- L’aperçu est désormais fidèle à l’export : la bande s’affiche dans l’éditeur avec exactement les couleurs de la vidéo exportée (fond, barre de lecture, texte, couleurs des personnages, ambiances, points karaoké), alors qu’elle paraissait jusqu’ici plus claire à l’écran. La barre de lecture de l’export accéléré par la carte graphique n’est plus d’un rouge différent.
- Presets de style : quatre presets fournis (Classique, Studio, Nuit, Contraste élevé), et les vôtres, enregistrés sous un nom et retrouvés dans tous les projets. Un style s’exporte en fichier `.coqstyle` à partager et s’importe depuis un tel fichier.
- Textures de point karaoké : le point qui rebondit sur les lignes karaoké peut être un rond (comme avant), un anneau, une étoile, un cœur, un losange, une note de musique, ou une image PNG de votre choix (« Image… »). Les formes prennent la couleur du personnage ; une image garde ses propres couleurs. Le choix vaut aussi pour le décompte avant la ligne, dans la bande comme dans l’export, et voyage avec les presets exportés. Avec « Deux images (sol / saut) », une image s’affiche quand le point touche le sol et une autre quand il est en l’air. Vous pouvez créer des groupes de points nommés (« Nouveau groupe ») ; puis clic droit sur une ligne karaoké → Karaoké pour donner au personnage le point par défaut, un groupe, ou un point qui n’appartient qu’à lui (forme ou image). Un personnage renommé garde son point ; supprimer un groupe remet ses personnages sur le point par défaut.
- Le style est enregistré dans le projet et commun à toutes ses langues.
- La bande de la vidéo exportée est maintenant identique à celle de l’éditeur : bords des lettres aussi nets (l’export accéléré par la carte graphique les assombrissait et les amincissait), marqueurs de boucle (barre, croix et numéro) et « out » (avec son libellé) aux mêmes couleurs et tailles, soulignement des répliques off ou partiellement off dans la couleur du texte, nom du personnage réduit pour tenir à côté de la réplique d’un autre personnage au lieu d’être rendu transparent, point karaoké rond avec son contour blanc et sous le texte, flèches de respiration avec leur pointe, icônes des comédiens encadrées et centrées comme dans l’éditeur. Dans l’éditeur, les marqueurs prennent les couleurs exactes de l’export.

## Barre de lecture

- La barre de lecture se déplace à la souris : on la saisit directement et on la glisse à gauche ou à droite, la bande suit en direct. Elle s’éclaircit au survol. Un double-clic dessus la recentre, `Échap` pendant le glissement annule. La position reste réglable dans les paramètres du projet.

## Export vidéo

- L’export d’une vidéo se fait maintenant en deux étapes. La fenêtre d’export sert d’abord à choisir les formats et réglages comme avant ; quand la vidéo MP4 est cochée, le bouton devient « Suivant… » et ouvre un écran « Mise en page de l’export ».
- Cet écran montre à droite un aperçu en direct de l’image exportée à la position de la tête de lecture : la vraie image de la vidéo et la vraie bande rythmo, dessinée comme dans le fichier final, sur le fond noir au format de sortie. La barre latérale de gauche règle la position X / Y, la largeur et la hauteur de la vidéo et de la bande séparément, ainsi que la taille de la bande et du texte karaoké ; l’aperçu suit chaque changement, et on peut aussi glisser la vidéo ou la bande à la souris dans l’aperçu.
- Un clic sur la vidéo ou la bande dans l’aperçu la sélectionne et affiche des poignées sur ses bords et ses coins (elles apparaissent aussi au survol, et le curseur change de forme) : tirer un bord gauche ou droit change la largeur, un bord haut ou bas la hauteur, un coin les deux. Le bord opposé reste en place pendant le redimensionnement ; le milieu du calque sert toujours à le déplacer.
- `Ctrl + Z` annule le dernier changement de mise en page (réglage, déplacement, redimensionnement — un glissement entier compte pour une seule étape —, réinitialisation, taille de la bande ou du texte karaoké) et `Ctrl + Y` ou `Ctrl + Maj + Z` le rétablit. Dans cet écran, ces raccourcis ne touchent jamais au projet ; le lecteur d’écran annonce ce qui a été rétabli.
- Le texte de la bande reste net quand on l’agrandit, même à 400 % : la bande est dessinée en vectoriel à la taille où elle sera affichée, puis seulement réduite, jamais étirée, dans l’aperçu comme dans la vidéo exportée.
- « Réinitialiser la mise en page » revient à la disposition habituelle (vidéo en haut, bande en bas sur toute la largeur), « Retour » revient aux réglages sans rien perdre, et « Lancer l’export » démarre l’export. Tout se fait aussi au clavier : `Tab` passe d’un réglage à l’autre, les flèches modifient la valeur, `Échap` revient à l’étape précédente.
- La mise en page (et les tailles de la bande et du texte karaoké) est mémorisée dans le projet, pour toutes ses langues : elle est retenue dès qu’on quitte cet écran, par « Retour », `Échap` ou « Lancer l’export », enregistrée avec le projet, et on la retrouve à la prochaine ouverture du projet sans avoir à la refaire. Avec une mise en page personnalisée, l’export passe par l’encodage logiciel.

# 5.2.1

## Édition

- Dans une réplique qui contient des points de synchronisation, écrire ou effacer du texte n’agit plus que sur la partie comprise entre deux limites : elle s’étire, tandis que les parties voisines et les points de synchronisation ne bougent pas. `Retour arrière` au début d’une partie et `Suppr` à sa fin ne mangent plus le texte de la partie voisine (une annonce vocale « Limite de synchronisation » l’indique).
- Sur une limite, le caret appartient à la partie d’où il vient : arrivé par la gauche (`→`), il écrit dans la partie de gauche ; arrivé par la droite (`←`), dans celle de droite. Un clic près d’une limite choisit la partie sous le pointeur. Sélectionner du texte à cheval sur une limite pour le supprimer ou le remplacer reste possible.
- Effacer la lettre ou la ponctuation à laquelle un point de synchronisation est accroché ne le décale plus d’un caractère, et taper une lettre identique à sa voisine juste à côté d’une limite ne dérègle plus la synchronisation.

## Détection

- Dans la palette `Alt + D` et les fiches de détection, une seconde description précise le cas d’usage et le placement de chaque indication, sans titre supplémentaire. Les panneaux adaptent leur taille au texte et à la largeur disponible, avec un retour automatique à la ligne. Le terme « dentale » est expliqué dans les indications concernées. Les explications s’appuient sur la charte de détection de l’UPAD et signalent les conventions variables ou propres à l’application. Elles sont également lues par le lecteur d’écran.
