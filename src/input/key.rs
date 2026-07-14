//! Normalized physical/logical keyboard input.

use winit::event::KeyEvent;
use winit::keyboard::{Key, ModifiersState, NamedKey};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InputWindow {
    Main,
    Secondary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Modifiers {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub logo: bool,
}

impl Modifiers {
    pub const NONE: Self = Self {
        ctrl: false,
        shift: false,
        alt: false,
        logo: false,
    };

    pub fn from_winit(state: ModifiersState) -> Self {
        Self {
            ctrl: state.control_key(),
            shift: state.shift_key(),
            alt: state.alt_key(),
            logo: state.super_key(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyCode {
    F5,
    Escape,
    Space,
    Tab,
    Delete,
    Backspace,
    Enter,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowDown,
    Character(char),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyStroke {
    pub key: KeyCode,
    pub modifiers: Modifiers,
    pub pressed: bool,
    pub repeat: bool,
    pub window: InputWindow,
}

impl KeyStroke {
    pub fn from_winit(event: &KeyEvent, modifiers: Modifiers, window: InputWindow) -> Option<Self> {
        let key = match &event.logical_key {
            Key::Named(named) => match named {
                NamedKey::F5 => KeyCode::F5,
                NamedKey::Escape => KeyCode::Escape,
                NamedKey::Space => KeyCode::Space,
                NamedKey::Tab => KeyCode::Tab,
                NamedKey::Delete => KeyCode::Delete,
                NamedKey::Backspace => KeyCode::Backspace,
                NamedKey::Enter => KeyCode::Enter,
                NamedKey::ArrowLeft => KeyCode::ArrowLeft,
                NamedKey::ArrowRight => KeyCode::ArrowRight,
                NamedKey::ArrowUp => KeyCode::ArrowUp,
                NamedKey::ArrowDown => KeyCode::ArrowDown,
                _ => return None,
            },
            Key::Character(text) => text.chars().next().map(|c| {
                // Shift is carried by Modifiers.  Normalizing the character
                // prevents Ctrl+Maj+Z from relying on casing.
                KeyCode::Character(c.to_ascii_lowercase())
            })?,
            _ => return None,
        };

        Some(Self {
            key,
            modifiers,
            pressed: event.state.is_pressed(),
            repeat: event.repeat,
            window,
        })
    }
}
