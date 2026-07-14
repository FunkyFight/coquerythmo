//! Compatibility exports for the rythmo workspace view.
//!
//! Rendering and interaction code lives with the `RythmoWorkspace` adapter;
//! this module keeps the existing UI call sites stable during the migration.

pub use crate::workspaces::rythmo::view::*;
