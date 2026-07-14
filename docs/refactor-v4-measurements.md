# Refactor V4 — mesures reproductibles

Ce document conserve les mesures structurelles du chantier. Les optimisations
ne sont acceptées qu'après comparaison sur les mêmes fixtures et les mêmes
commandes.

## Baseline figée

La baseline du plan, avant migration, est :

- `cargo test --locked` : 89 tests réussis ;
- cible binaire initiale : 112 avertissements ;
- `src/main.rs` : 1 583 lignes ;
- `src/state.rs` : 3 538 lignes ;
- `src/ui/mod.rs` : 3 587 lignes ;
- `src/ui/rythmo.rs` : 6 778 lignes ;
- `src/rythmo_gpu_renderer.rs` : 2 962 lignes ;
- `src/rythmo_cpu_renderer.rs` : 1 587 lignes ;
- `src/video_export.rs` : 1 776 lignes.

## Commandes de mesure

Depuis la racine du dépôt :

```powershell
cargo test --locked
cargo test --release --locked
cargo clippy --all-targets --release --locked -- -D warnings
```

Pour les tailles de fichiers :

```powershell
@( 'src/main.rs', 'src/state.rs', 'src/ui/mod.rs', 'src/ui/modal_host.rs',
   'src/ui/shell.rs', 'src/workspaces/rythmo/view.rs', 'src/workspaces/rythmo/state.rs',
   'src/workspaces/rythmo/geometry.rs', 'src/workspaces/rythmo/controller.rs',
   'src/rythmo_gpu_renderer.rs', 'src/rythmo_cpu_renderer.rs',
   'src/video_export/mod.rs', 'src/video_export/pipeline.rs',
   'src/video_export/capabilities.rs', 'src/video_export/progress.rs',
   'src/video_export/types.rs', 'src/video_export/audio.rs',
   'src/video_export/frame_source.rs', 'src/video_export/ffmpeg.rs' ) | ForEach-Object {
    "$_`t$((Get-Content $_).Count)"
}
```

Les fixtures de référence sont dans `tests/fixtures/`. Les mesures de frame,
de lignes visitées, de scènes construites et d'export court devront être
ajoutées ici avant toute optimisation de la phase 8.

## Mesure intermédiaire après les extractions et la scène commune

Au 14 juillet 2026, le dépôt contient 103 tests unitaires, 4 tests
d'intégration et 0 doctest passants avec `cargo test --locked`.
Les tailles observées sont :

- `src/main.rs` : 4 lignes ;
- `src/state.rs` : 3 159 lignes ;
- `src/ui/mod.rs` : 2 355 lignes ;
- `src/ui/modal_host.rs` : 832 lignes ;
- `src/ui/shell.rs` : 645 lignes ;
- `src/workspaces/rythmo/view.rs` : 4 168 lignes ;
- `src/workspaces/rythmo/state.rs` : 495 lignes ;
- `src/workspaces/rythmo/geometry.rs` : 498 lignes ;
- `src/workspaces/rythmo/controller.rs` : 202 lignes ;
- `src/workspaces/rythmo/drawing.rs` : 126 lignes ;
- `src/workspaces/rythmo/text_controller.rs` : 32 lignes ;
- `src/workspaces/rythmo/drag.rs` : 105 lignes ;
- `src/workspaces/rythmo/mouse.rs` : 203 lignes ;
- `src/workspaces/rythmo/mouse_buttons.rs` : 170 lignes ;
- `src/workspaces/rythmo/syllable.rs` : 173 lignes ;
- `src/workspaces/rythmo/press.rs` : 183 lignes ;
- `src/workspaces/rythmo/selection.rs` : 286 lignes ;
- `src/workspaces/rythmo/keyboard.rs` : 100 lignes ;
- `src/workspaces/rythmo/keyboard_nav.rs` : 212 lignes ;
- `src/rythmo_gpu_renderer.rs` : 2 787 lignes ;
- `src/rythmo_cpu_renderer.rs` : 1 438 lignes ;
- `src/video_export/mod.rs` : 21 lignes de module ;
- `src/video_export/pipeline.rs` : 373 lignes ;
- `src/video_export/capabilities.rs` : 209 lignes ;
- `src/video_export/progress.rs` : 156 lignes ;
- `src/video_export/types.rs` : 131 lignes ;
- `src/video_export/audio.rs` : 288 lignes ;
- `src/video_export/frame_source.rs` : 351 lignes ;
- `src/video_export/ffmpeg.rs` : 347 lignes.

La baisse de taille est structurelle et ne constitue pas à elle seule un gain
de performance. Le profil release reste volontairement laissé à la machine
de validation finale ; la validation visuelle/smoke de l'application reste
également à faire.
