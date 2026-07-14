//! Application composition and the native event loop.

mod bootstrap;
mod dispatcher;
mod event_loop;
mod file_picker;

/// Start the existing application event loop.
pub fn run() {
    event_loop::run();
}
