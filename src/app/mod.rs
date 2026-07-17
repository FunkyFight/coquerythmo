//! Application composition and the native event loop.

mod bootstrap;
mod dispatcher;
mod event_loop;
mod file_picker;

/// Start the existing application event loop.
pub fn run(startup_path: Option<std::path::PathBuf>) {
    event_loop::run(startup_path);
}
