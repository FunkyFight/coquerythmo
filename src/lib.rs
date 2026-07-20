//! Testable product modules for Coquerythmo.
//!
//! The binary is intentionally kept as a thin process entry point.  Runtime
//! composition belongs to [`app`]; the existing product modules remain
//! private to the crate while their unit tests run through this library target.

pub mod accessibility;
pub mod app;
pub mod application;
pub mod audio_transfer;
pub mod automation;
pub mod command;
pub mod config;
pub mod configured_export;
pub mod constants;
pub mod delivery_export;
pub mod detection;
mod detection_runtime;
pub mod export;
pub mod graphics;
pub mod i18n;
pub mod input;
mod integrity;
pub mod media_binary;
pub mod media_recording;
pub mod network;
pub mod observer;
pub mod packet;
pub mod platform;
pub mod project;
pub mod project_archive;
pub mod project_metadata;
pub mod recording;
pub mod recording_mix;
pub mod recording_runtime;
pub mod render_index;
pub mod rendering;
pub mod rythmo_cpu_renderer;
pub mod rythmo_drawing;
pub mod rythmo_gpu_renderer;
pub mod rythmo_layout;
pub mod rythmo_line;
pub mod state;
mod state_detection;
pub mod syllable;
pub mod ui;
pub mod update;
pub mod vector_text;
pub mod video;
pub mod video_export;
pub mod video_proxy;
pub mod voice_actor;
pub mod workspaces;
