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
@( 'src/main.rs', 'src/state.rs', 'src/ui/mod.rs', 'src/ui/rythmo.rs',
   'src/workspaces/rythmo/view.rs', 'src/workspaces/rythmo/state.rs',
   'src/workspaces/rythmo/geometry.rs', 'src/workspaces/rythmo/controller.rs',
   'src/rythmo_gpu_renderer.rs', 'src/rythmo_cpu_renderer.rs',
   'src/video_export.rs' ) | ForEach-Object {
    "$_`t$((Get-Content $_).Count)"
}
```

Les fixtures de référence sont dans `tests/fixtures/`. Les mesures de frame,
de lignes visitées, de scènes construites et d'export court devront être
ajoutées ici avant toute optimisation de la phase 8.

## Mesure intermédiaire après les extractions et la scène commune

Au 14 juillet 2026, le dépôt contient 103 tests de bibliothèque passants.
Les tailles observées sont :

- `src/main.rs` : 4 lignes ;
- `src/state.rs` : 2 964 lignes ;
- `src/ui/mod.rs` : 3 561 lignes ;
- `src/ui/rythmo.rs` : 6 lignes de façade ;
- `src/workspaces/rythmo/view.rs` : 5 191 lignes ;
- `src/workspaces/rythmo/state.rs` : 482 lignes ;
- `src/workspaces/rythmo/geometry.rs` : 484 lignes ;
- `src/workspaces/rythmo/controller.rs` : 311 lignes ;
- `src/workspaces/rythmo/text_controller.rs` : 33 lignes ;
- `src/workspaces/rythmo/drag.rs` : 108 lignes ;
- `src/workspaces/rythmo/mouse.rs` : 199 lignes ;
- `src/rythmo_gpu_renderer.rs` : 2 785 lignes ;
- `src/rythmo_cpu_renderer.rs` : 1 434 lignes ;
- `src/video_export.rs` : 1 776 lignes.

Cette baisse de taille n'est pas présentée comme un gain de performance :
les benchmarks release et la validation visuelle restent à faire.
