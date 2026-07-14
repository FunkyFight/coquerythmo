//! Video export pipeline.
//!
//! The implementation is split into capability probing, pipeline orchestration,
//! frame production, audio muxing, FFmpeg invocation and progress reporting.

mod audio;
mod capabilities;
mod ffmpeg;
mod frame_source;
mod pipeline;
pub mod preroll;
mod progress;
mod types;

pub const EXPORT_RENDER_BACKEND_UNKNOWN: u32 = 0;
pub const EXPORT_RENDER_BACKEND_GPU: u32 = 1;
pub const EXPORT_RENDER_BACKEND_CPU: u32 = 2;
pub const EXPORT_CANCELLED_MESSAGE: &str = "Export canceled";

pub use capabilities::check_ffmpeg;
pub use pipeline::export_mp4;
pub use progress::is_cancelled_error;
