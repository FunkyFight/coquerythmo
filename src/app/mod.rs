//! Application composition and the native event loop.

mod bootstrap;
pub(crate) mod dispatcher;
mod event_loop;
pub(crate) mod file_picker;

/// What to load when the app starts. Either a local project file or a
/// `coquerythmo://` protocol URL invoked from a browser/shortcut.
#[derive(Debug)]
pub enum StartupInput {
    /// Path to a `.coquerythmo` project file (double-click on file).
    Project(std::path::PathBuf),
    /// Full `coquerythmo://...` URI received by the Windows protocol handler.
    Url(String),
}

/// Start the existing application event loop.
pub fn run(startup: Option<StartupInput>) {
    event_loop::run(startup);
}
