# Moteur texte → phonèmes → signes

Le moteur de génération des signes de détection est entièrement déterministe et
ne consulte jamais l’audio. La chaîne d’exécution est :

```text
texte affiché
  → normalize_line
  → tokenize
  → dictionnaire / exceptions / règles grapheme→phonème
  → mapping phonème→DetectionKind
  → DetectionCue positionné sur la ligne
```

Le point d’entrée linguistique est `converter_for_dialect(Language, Dialect)`.
Les trois langues utilisent le même `GraphemeToPhonemeConverter`, le même
`PhoneticLine` et le même générateur de cues.

## Coordonnées et placement

`TextRange` est une plage `[start, end)` d’indices de graphèmes dans le texte
original affiché. La normalisation peut développer une ligature (`ﬁ`, `œ`),
réduire les espaces ou uniformiser les apostrophes, mais la carte inverse de
`NormalizedLine` reconduit chaque segment vers le texte visible.

`GraphemeSegment` conserve le groupe écrit, sa plage, ses phonèmes, son état
muet et les marqueurs optionnels. Un phonème issu d’un groupe de plusieurs
lettres hérite de toute la plage. Les phonèmes multiples d’un même groupe sont
répartis seulement avec `SignPlacement::Distributed` ; l’action utilisateur
utilise `Center`.

La position temporelle initiale est une interpolation uniforme entre
`start_frame` et `start_frame + duration_frames`, à partir de la plage de
graphèmes. `GenerationOptions::progress` permet de fournir ultérieurement une
progression visuelle issue des points de synchronisation existants. Il n’existe
aucun alignement forcé, ASR ou analyse de la piste sonore.

## Inventaires linguistiques

Les listes ci-dessous sont les inventaires effectivement couverts par
`mapping::relevant_phonemes`. Les tables `RULES`, `DICTIONARY` et `EXCEPTIONS`
dans les modules de langue sont la source de vérité pour les graphèmes et les
exemples. Le test `inventory_check` échoue si un phonème pertinent n’a pas de
mapping explicite ou de mot-sonde.

### Français

```text
Voyelles : OpenCentral, OpenBackFr, CloseFront, CloseMidFront,
OpenMidFront, CloseMidBackRounded, OpenMidBackRounded, CloseBackRounded,
CloseFrontRounded, CloseMidFrontRounded, OpenMidFrontRounded, Schwa.
Nasales : NasalOpenBackFr, NasalOpenMidFrontFr, NasalOpenMidBackFr,
NasalOpenMidFrontRoundedFr.
Consonnes et glides : VoicelessBilabialPlosive, VoicedBilabialPlosive,
VoicelessAlveolarPlosive, VoicedAlveolarPlosive, VoicelessVelarPlosive,
VoicedVelarPlosive, BilabialNasal, AlveolarNasal, PalatalNasal,
VoicelessLabiodentalFricative, VoicedLabiodentalFricative,
VoicelessAlveolarFricative, VoicedAlveolarFricative,
VoicelessPostalveolarFricative, VoicedPostalveolarFricative,
AlveolarLateralApproximant, PalatalApproximant,
LabialPalatalApproximant, LabialVelarApproximant, VoicedUvularFricative,
VoicelessPostalveolarAffricate, VoicedPostalveolarAffricate, VelarNasal.
```

Les tables françaises traitent notamment `ch`, `ph`, `gn`, `ill`, `qu`, `gu`,
`ge/gi`, `sc`, les groupes vocaliques (`ai`, `au`, `eau`, `eu`, `œu`, `ou`,
`oi`, `oin`, `on`, `an/en`, `in/ain/ein`, `un`), le `e` muet, les finales,
les élisions, les nombres, symboles, acronymes et les emprunts courants.

### Anglais

```text
Voyelles : OpenCentral, CloseFront, CloseMidFront, OpenMidFront,
OpenMidBackRounded, CloseBackRounded, Schwa, NearCloseFrontEn,
NearCloseBackRoundedEn, OpenMidBackEn, NearOpenFrontEn,
OpenBackRoundedEnGb, RColoredSchwaEnUs, OpenMidCentralEnGb,
RColoredOpenMidCentralEnUs.
Diphtongues : DiphthongFaceEn, DiphthongPriceEn, DiphthongChoiceEn,
DiphthongGoatEn, DiphthongMouthEn, DiphthongNearEnGb,
DiphthongSquareEnGb, DiphthongCureEnGb.
Consonnes : VoicelessBilabialPlosive, VoicedBilabialPlosive,
VoicelessAlveolarPlosive, VoicedAlveolarPlosive, VoicelessVelarPlosive,
VoicedVelarPlosive, BilabialNasal, AlveolarNasal, VelarNasal,
VoicelessLabiodentalFricative, VoicedLabiodentalFricative,
VoicelessDentalFricative, VoicedDentalFricative,
VoicelessAlveolarFricative, VoicedAlveolarFricative,
VoicelessPostalveolarFricative, VoicedPostalveolarFricative,
VoicelessPostalveolarAffricate, VoicedPostalveolarAffricate,
VoicelessGlottalFricative, AlveolarLateralApproximant,
AlveolarApproximant, PalatalApproximant, LabialVelarApproximant,
SyllabicL, SyllabicN, SyllabicM, GlottalStop,
VoicelessLabialVelarFricative, AlveolarTap.
```

`Dialect::EnUs` et `Dialect::EnGb` sélectionnent les entrées adaptées :
`r` rhotique ou non rhotique, `lot`, `nurse`, `near`, `square`, `cure`, le
flap américain, les voyelles réduites et les variantes de `th`. Les
homographes (`read`, `lead`, `wind`, `live`, `close`, `record`, `present`,
`object`, `content`) restent plusieurs candidats ; le premier est choisi par
défaut et `selected_candidate` permet une correction ultérieure.

### Espagnol

```text
Voyelles : OpenCentral, CloseFront, CloseMidFront, CloseMidBackRounded,
CloseBackRounded.
Consonnes et approximantes : VoicelessBilabialPlosive,
VoicedBilabialPlosive, VoicedBilabialApproximant,
VoicelessAlveolarPlosive, VoicedAlveolarPlosive, VoicedDentalApproximant,
VoicelessVelarPlosive, VoicedVelarPlosive, VoicedVelarApproximant,
BilabialNasal, AlveolarNasal, PalatalNasal,
VoicelessLabiodentalFricative, VoicelessAlveolarFricative,
VoicelessDentalFricativeEsSpain, VoicelessVelarFricative,
VoicelessPostalveolarAffricate, VoicedPalatalFricative,
VoicedPostalveolarFricativeEs, VoicelessPostalveolarFricativeEs,
VoicelessAlveolarAffricate, AlveolarLateralApproximant,
PalatalApproximant, LabialVelarApproximant, AlveolarTap, AlveolarTrill.
```

`Dialect::EsSpain` active la distinction `z/ce/ci → θ` ;
`Dialect::EsLatam` utilise le seseo. Les variantes de `ll/y` sont conservées
comme candidats alternatifs sur les mots concernés et les emprunts comme
`tsunami` couvrent les attaques non standard.

## Mapping vers les signes

`DetectionMapping` est indépendant des convertisseurs linguistiques. Un
`PhonemeSignRule` contient une séquence, une liste de `DetectionKind` et une
priorité. Une séquence plus prioritaire, puis la plus longue à priorité égale,
gagne. La génération regroupe ensuite les phonèmes par syllabe et conserve le
geste dominant : une fermeture labiale reste prioritaire, tandis qu’une voyelle
arrondie domine les consonnes non labiales de sa syllabe. Le mapping par défaut
regroupe notamment :

```text
p/b/m et les nasales bilabiales       → Labial
f/v et les réalisations labiales       → SemiLabial
voyelles ouvertes et frontales         → OpeningWave / TeethVisible
voyelles arrondies et /w/              → ForwardWave
t/d/s/z/l/k/g et fricatives dentales  → TeethVisible
fermetures sans autre geste dominant   → MouthClosed
```

`mapping_fingerprint` est stocké dans `GeneratedSignsInfo`. Si un profil
personnalisé est introduit, il doit être validé par `validate_mapping` avant
de générer des cues.

## Persistance, édition et undo

Une génération crée une `DetectionChange::Batch` contenant tous les `Add` et
le `SetGeneratedSigns`. Le batch est atomique et correspond à une seule entrée
d’historique : un undo retire donc tous les cues de la ligne en une fois.

Les cues générés sont des `DetectionCue` normaux : ils sont persistés dans
`ProjectSettings::detections`, rendus au-dessus de la ligne, sélectionnables,
déplaçables, redimensionnables et supprimables avec les outils existants.
`GeneratedSignsInfo` conserve la langue, le dialecte, la version du moteur,
l’empreinte du texte, l’empreinte du mapping et les ids des cues. Une
suppression manuelle retire l’id correspondant de cette liste ; le système ne
réécrit pas silencieusement une génération existante.

L’action est accessible dans le menu contextuel d’une ligne sous
« Convertir en signes de détection ». Le projet conserve le choix de langue
Français, Anglais ou Espagnol dans le réglage de langue syllabique ; l’accent
anglais ou espagnol par défaut est ensuite utilisé par le convertisseur.

## Ajouter ou modifier des données

1. Ajouter un variant à `Phoneme` avec un identifiant stable, une notation IPA
   et une description, puis l’ajouter à `Phoneme::ALL`.
2. Ajouter ce variant à `relevant_phonemes` de la langue concernée.
3. Ajouter les règles contextuelles dans `RULES`, les mots réguliers dans
   `DICTIONARY` et les irrégularités dans `EXCEPTIONS`. Chaque entrée doit
   concaténer exactement la clé du mot ; `validate_dict_table` le vérifie.
4. Ajouter ou modifier le groupe correspondant dans `default_mapping`.
5. Ajouter un mot-sonde dans `inventory_check::probe_words` et un test ciblé.

Pour une nouvelle langue, créer un module de données et un wrapper G2P,
ajouter `Language`/`Dialect`, puis brancher uniquement le constructeur dans
`converter_for_dialect`. Le reste du pipeline et du format de persistance ne
change pas.

## Vérification

```text
cargo test --lib phonetics
```

Le rapport de couverture vérifie les mappings, les mots-sondes, les règles,
les dictionnaires et une phrase de fumée par dialecte. Les plages Unicode,
les règles séquentielles et le batch undo/persistance possèdent également des
tests de régression dédiés.

## Limites connues

La génération rapide actuelle utilise le dialecte et le mapping par défaut,
le placement centré et exclut les phonèmes optionnels. `GenerationResult`
fournit déjà les plages inconnues, ambiguës et optionnelles pour une future
fenêtre de prévisualisation ; l’action directe affiche ces compteurs dans le
toast et refuse de remplacer une génération existante. Les dictionnaires
utilisateur et les corrections persistées par occurrence/projet sont prévus
par `PronunciationSource` et `selected_candidate`, mais leur écran d’édition
n’est pas encore branché.
