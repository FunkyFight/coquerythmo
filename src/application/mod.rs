//! Application-level commands and use-case boundaries.

pub mod collaboration_service;
pub mod command;
pub mod context;
pub mod delta_codec;
pub mod detection_service;
#[path = "edit_service.rs"]
mod edit_service_base;
#[path = "edit_service_facade.rs"]
pub mod edit_service;
pub mod job_service;
pub mod playback_service;
pub mod project_service;
pub mod render_service;
pub mod ui_shell;
pub mod window_service;
pub mod workspace_service;
