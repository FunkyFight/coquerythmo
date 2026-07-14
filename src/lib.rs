//! Testable product modules for Coquerythmo.
//!
//! The binary is intentionally kept as a thin process entry point.  Runtime
//! composition belongs to [`app`]; the existing product modules remain
//! private to the crate while their unit tests run through this library target.

pub mod app;
pub mod application;
pub mod command;
pub mod config;
pub mod constants;
pub mod export;
pub mod graphics;
pub mod i18n;
pub mod input;
pub mod media_binary;
pub mod network;
pub mod observer;
pub mod packet;
pub mod platform;
pub mod project;
pub mod render_index;
pub mod rendering;
pub mod rythmo_cpu_renderer;
pub mod rythmo_drawing;
pub mod rythmo_gpu_renderer;
pub mod rythmo_layout;
pub mod rythmo_line;
pub mod state;
pub mod syllable;
pub mod ui;
pub mod update;
pub mod vector_text;
pub mod video;
pub mod video_export;
pub mod video_proxy;
pub mod voice_actor;
pub mod workspaces;
