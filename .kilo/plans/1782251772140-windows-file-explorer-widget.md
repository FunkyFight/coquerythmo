# Windows-Like Integrated File Explorer Widget Plan

## Goal
Replace every native file open/save dialog used by Coquerythmo with an integrated Windows-like picker modal rendered inside the existing custom UI. The first version is a single-file picker, not a full destructive file manager.

## Confirmed Decisions
- Target platform: Windows only for this widget.
- Visual style: keep Coquerythmo's dark modal style, while matching Windows Explorer layout and behavior.
- Scope: replace all current `file_dialog::open_file` / `save_file` usage with an integrated picker.
- Include: sidebar locations, Windows drives, address/breadcrumb path, back/forward/up navigation, file/folder details list, extension filters, filename field for save, overwrite confirmation, and `Nouveau dossier`.
- Exclude for the first version: delete, rename, copy/paste, destructive context menus, thumbnails/preview pane, and full Explorer search.

## Current Architecture Notes
- Native dialogs are centralized in `src/file_dialog.rs`, but calls are synchronous from `handle_action` in `src/main.rs`.
- The UI already supports blocking custom modals through `src/ui/mod.rs` and modal modules such as `export_modal.rs`, `voice_actor_modal.rs`, and `save_prompt_modal.rs`.
- `StartExport` currently opens a save dialog inside a worker thread before MP4 export starts. The integrated picker must choose the output path before spawning the export thread.
- Browse buttons inside existing modals must keep their parent modal alive while the file picker is displayed above it.

## Implementation Tasks
1. Add a new UI module `src/ui/file_explorer_modal.rs`.
   - Define `FileExplorerModal`, `FileExplorerMode::{Open, Save}`, `FileExplorerResult`, `FileFilterSpec`, and a typed `FilePickerIntent` or equivalent result context.
   - Store owned filter names/extensions so picker requests can outlive the originating action.
   - Model selection state, current directory, history back/forward stacks, scroll offset, selected filter, typed filename, active text field, overwrite confirmation, errors, and pending directory scan state.

2. Implement Windows filesystem helpers.
   - Enumerate drives with Windows API FFI such as `GetLogicalDrives` or equivalent.
   - Use `dirs` for common sidebar entries: Desktop, Documents, Downloads, Home, plus current/project/video folders when relevant.
   - Read directories with `std::fs::read_dir`, gather metadata for type, size, modified time, directory/file distinction, and access errors.
   - Sort folders first, then files by name by default.
   - Apply active filters to files while always showing directories.
   - Use background directory scans with a generation id and channel so large folders do not freeze the render loop.

3. Render the modal in the existing UI system.
   - Add `pub mod file_explorer_modal;` and `file_explorer_modal: Option<FileExplorerModal>` to `Ui`.
   - Render it as the topmost modal, after parent modals and before/to the same priority as hard-blocking overlays as appropriate.
   - Layout: dim background, large centered card, title, toolbar buttons, address/breadcrumb field, sidebar, details list, filter dropdown, filename/path field, primary/secondary buttons, status/error text.
   - Draw simple folder/file/drive icons using quads/labels or existing icon infrastructure; do not depend on OS thumbnails.
   - Virtualize visible rows so directories with many files do not create thousands of labels/quads.

4. Route input through the modal stack.
   - Make the file explorer intercept events before settings/export/voice actor/proxy/server modals when it is open.
   - Include file explorer in `Ui::is_editing_text`, `needs_background_poll`, `next_cursor_blink_deadline`, and any redraw scheduling needed for async scans/cursor blink.
   - Handle mouse: row select, double-click folder/file, sidebar location, toolbar buttons, filter dropdown, scroll list, buttons.
   - Handle keyboard: Escape cancel, Enter open/select/save, Backspace parent when not editing text, arrow up/down row navigation, basic text entry for address/filename, and Ctrl+A/C/V behavior where existing event forwarding supports it.
   - If additional named keys are required, update `main.rs` key mapping minimally.

5. Replace native dialog calls with picker requests.
   - Remove direct `file_dialog::open_file` / `save_file` calls from `handle_action`.
   - Convert each current dialog entry point into an `OpenFilePicker`-style action or direct `State` method that opens `Ui::open_file_explorer(...)` with intent and filters.
   - Cover all current usages:
     - Add video: `mp4`, `mov`, `avi`, `mkv`, `webm`.
     - Import Coquerythmo JSON.
     - Import Cappela DETX.
     - Import SRT.
     - Export project JSON.
     - Quick save when no `project_path`.
     - Voice actor icon image selection.
     - Export modal instrumental audio selection.
     - MP4 output path selection.
     - New project save flow.
   - Keep existing initial directory rules: project/video parent first when available, otherwise Downloads/Home.

6. Add picker result actions and intent handling.
   - Add a `UiAction` variant for picker completion, for example `FilePickerSelected { intent, path }`, and cancellation if needed.
   - On selection, execute the same side effects currently performed after native dialogs.
   - Preserve default extension behavior for save paths when the typed filename has no extension.
   - For overwrite: if target exists in save mode, show an internal confirmation before returning the selected path.
   - For `Nouveau dossier`: create the folder in the current directory, refresh the listing, select/navigate to it, and show inline error text on failure.

7. Fix MP4 export flow.
   - Change `UiAction::StartExport { ... }` so it opens the save picker with a pending MP4 export payload instead of spawning a thread that opens a dialog.
   - Add a post-picker action, for example `StartExportToPath { output_path, fps, br_scale, karaoke_text_scale, export_width, export_height, instrumental_audio_path }`.
   - Spawn the MP4 export worker only after the integrated picker returns a valid path.
   - Keep the progress/cancel behavior unchanged after export starts.

8. Preserve parent modal flows.
   - When browsing instrumental audio from `ExportModal`, keep `export_modal` open underneath the file explorer and set its path on picker completion.
   - When browsing voice actor icons, keep `voice_actor_modal` open and set its icon path on picker completion.
   - When saving before `NewProject`, if a save path must be chosen, complete the save first, then reset and open the Add Video picker.

9. Remove or retire the old file dialog module.
   - Remove `mod file_dialog` and stale helper types if no longer referenced.
   - `rfd` may remain because `src/update.rs` still uses `rfd::MessageDialog`; do not remove the dependency unless update handling is also replaced.

10. Add French i18n keys in `i18n/fr.toml`.
   - Add labels for file explorer title variants, sidebar, buttons, columns, filters, overwrite confirmation, new folder, errors, loading, empty directory, and inaccessible directory.

## Validation Plan
- Run `cargo check`.
- Manually test every replaced flow:
  - Add video from Downloads and from a project-adjacent folder.
  - Import JSON, DETX, and SRT.
  - Export project JSON with and without extension, including overwrite confirmation.
  - Ctrl+S quick save with existing `project_path` and without one.
  - New project with dirty unsaved project: save, discard, cancel, then chained Add Video.
  - Export MP4: choose output path, confirm progress starts only after path selection, cancel export still works.
  - Export modal instrumental audio browse returns to the still-open export modal.
  - Voice actor icon browse returns to the still-open voice actor modal.
  - Navigate drives, Downloads, parent folders, inaccessible directories, empty directories, and large directories.
  - Create a new folder and verify listing refresh/error handling.
  - Window resize while picker is open.

## Risks And Mitigations
- Large folders can freeze UI if scanned synchronously: use background scan generation and render loading state.
- Existing event loop does not forward every Windows Explorer shortcut: implement essential keyboard behavior first and keep advanced shortcuts out of scope.
- Modal stacking can regress export/voice actor flows: make file explorer topmost and return typed intent results to the parent state.
- MP4 export currently chooses path inside a worker: explicitly split path selection from export execution.
- Save-before-new-project can require chained actions: model continuation intent instead of trying to run synchronous save logic.

## Open Questions
None for the first implementation pass.
