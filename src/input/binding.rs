//! Declarative context + keystroke bindings.

use super::context::InputContext;
use super::key::{KeyCode, KeyStroke, Modifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatPolicy {
    PressOnly,
    PressAndRepeat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyPattern {
    pub key: KeyCode,
    pub modifiers: Modifiers,
    pub repeat: RepeatPolicy,
}

impl KeyPattern {
    pub fn matches(&self, stroke: &KeyStroke) -> bool {
        self.key == stroke.key
            && self.modifiers == stroke.modifiers
            && stroke.pressed
            && (self.repeat == RepeatPolicy::PressAndRepeat || !stroke.repeat)
    }
}

#[derive(Debug, Clone)]
pub struct Binding<C> {
    pub context: InputContext,
    pub pattern: KeyPattern,
    pub command: C,
}
