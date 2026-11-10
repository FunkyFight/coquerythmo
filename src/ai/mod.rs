//! Needle 3, the local assistant of the rythmo band.
//!
//! `Ctrl + Entrée` opens a floating prompt; the query goes to the Needle 3
//! engine (`needle.exe` + `needle3.cact`, see [`needle`]), which answers with
//! tool calls. Every tool is declared once in [`tools`] (the catalogue sent to
//! the engine) and executed in [`dispatch`], which turns a call into the same
//! `UiAction`s the keyboard and the mouse produce, so every edit stays
//! undoable, replicated and announced.
//!
//! The engine only sees a handful of tools per query: [`retrieval`] picks the
//! ones whose trigger words appear in the query, because the model's context
//! is 8192 tokens and its accuracy drops with every extra schema.
//!
//! `TOOLS_NEEDLE3.md` lists every tool and must stay in sync with [`tools`].

pub mod colors;
pub mod dispatch;
pub mod needle;
pub mod query;
pub mod retrieval;
pub mod session;
pub mod timecode;
pub mod tools;
pub mod training;
