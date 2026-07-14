# Refactor V4 — extension boundaries

Le refactor conserve une seule implémentation produit :
`RythmoWorkspace`, enregistrée par `State` dans `WorkspaceHost`. Aucun écran
d'onglets ni sélecteur d'espace n'est ajouté.

Le shell UI conserve les zones communes dans `ui::Ui`, tandis que les
politiques de géométrie de timeline et de split sont dans `ui::shell`. Les
instances, transitions, outcomes et couches de rendu des modales sont
centralisés par `ui::modal_host::ModalHost`; le shell ne possède plus un
handler par modale.

Les collections mutables de `Project` sont encapsulées par le domaine. Les
lecteurs exposent des vues immuables (`markers()`, `known_characters()`,
`voice_actors()`, `drawing()` et `settings()`), tandis que les changements
passent par les méthodes de domaine ou par `EditExecutor`, qui reste la porte
d'entrée de l'historique, de `dirty` et des origines réseau/import.

## Ajouter un espace plus tard

Un nouvel espace devra rester limité à un module sous `src/workspaces/` et à
son enregistrement dans la racine de composition. Il implémentera le contrat
`application::workspace_service::Workspace` et exposera seulement :

- une identité stable et un contexte d'entrée ;
- un modèle de toolbar contextuelle ;
- la gestion d'événements de contenu ;
- l'indication de redraw.

Il ne devra pas recevoir `State`, `Ui` ou une collection de projets. Le test
`workspace_host_exposes_one_active_workspace` montre le contrat avec un faux
workspace sans fenêtre ni GPU.

## Ajouter un binding contextuel

Un binding doit être déclaré dans la table du propriétaire, via
`input::router::ShortcutRouter`, avec une paire `InputContext` + `KeyPattern`.
Le routeur parcourt la pile dans l'ordre de priorité et applique la politique
de répétition de `RepeatPolicy`. Il ne doit pas contenir de condition sur un
identifiant de workspace.

Avant de déplacer un raccourci, ajouter sa ligne à la table de caractérisation
de `input::router::tests::existing_shortcuts_characterization_table`, puis
vérifier les fenêtres principale et secondaire ainsi que l'appui répété.
