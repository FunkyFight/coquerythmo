# Fine-tuning de Needle 3

Le modèle Needle 3 (121 M de paramètres) est petit : il choisit mal entre
plusieurs tools, retient des appels faute de confiance et réécrit les
arguments. Un adaptateur LoRA entraîné sur le catalogue de Coquerythmo
l'habitue aux phrases réelles des utilisateurs.

Les données d'entraînement sont produites **par le code de l'app**, pour que
le modèle voie exactement ce qu'il verra : mêmes tools sélectionnés
(`src/ai/retrieval.rs`), même texte de session (`SessionFacts`).

## Étapes

Dossier de travail : `D:\Données\Documents\Needle` (checkpoint de base dans
`checkpoints/`, données et résultats dans `finetune/`).

1. Phrases et appels attendus (modèles par tool, FR / EN / ES, variations de
   casse, d'accents et de politesse) :

   ```bash
   python tools/needle3_finetune/generate_queries.py "D:/Données/Documents/Needle/finetune/queries.jsonl"
   ```

2. Fichier d'entraînement, écrit par l'app ; les phrases dont le tool attendu
   n'est pas envoyé au moteur vont dans `train.jsonl.missed.jsonl` (déclencheurs
   à ajouter dans `src/ai/tools.rs`) :

   ```bash
   NEEDLE_TRAIN_IN=".../queries.jsonl" NEEDLE_TRAIN_OUT=".../train.jsonl" cargo test --lib ai::training -- --ignored --nocapture
   ```

3. Entraînement (package Python `needle` 3.2.2, jax). Les exemples font moins
   de 512 tokens ; des lots de 8 tiennent en mémoire sur CPU :

   ```bash
   needle finetune train.jsonl --checkpoint ../checkpoints/needle3.safetensors --epochs 3 --batch-size 8 --lr 2e-4 --max-len 512 --out coquerythmo_lora.safetensors
   ```

   Sous Windows, jax ne tourne que sur CPU : compter plusieurs heures.

4. Export du modèle affiné en `.cact` :

   ```bash
   needle build ../checkpoints/needle3.safetensors --lora coquerythmo_lora.safetensors --out needle3-coquerythmo.cact
   ```

   Un entraînement local n'entraîne pas la tête de confiance : le modèle
   affiné ne rend plus de `confidence` ni d'appels retenus.

5. Comparaison avec le modèle de base sur des phrases écrites à la main,
   jamais vues à l'entraînement (dont des phrases réelles d'utilisateurs) :

   ```bash
   python tools/needle3_finetune/evaluate.py ".../finetune/eval" src/ai/needle3 src/ai/needle3/needle3.cact ".../needle3-coquerythmo.cact"
   ```

6. Si le modèle affiné est meilleur, le copier en `src/ai/needle3/needle3.cact`
   (l'app et le packaging lisent ce nom).

Après tout ajout ou changement de tool : reprendre à l'étape 1.
