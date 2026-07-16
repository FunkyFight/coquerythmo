//! Normalized physical/logical keyboard input.

use winit::event::KeyEvent;
use winit::keyboard::{
    Key, KeyCode as WinitKeyCode, KeyLocation, ModifiersState, NamedKey, PhysicalKey,
};

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
    F10,
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
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,
    NumpadSubtract,
    Character(char),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyStroke {
    /// Normalized logical key used for bindings.
    pub key: KeyCode,
    /// Raw physical key retained for layout-independent bindings and diagnostics.
    pub physical_key: Option<WinitKeyCode>,
    pub location: KeyLocation,
    pub modifiers: Modifiers,
    pub pressed: bool,
    pub repeat: bool,
    pub window: InputWindow,
}

impl KeyStroke {
    pub fn from_winit(event: &KeyEvent, modifiers: Modifiers, window: InputWindow) -> Option<Self> {
        // The logical values for numpad digits are indistinguishable from the
        // top number row. Resolve those physical keys first so shortcuts never
        // depend on NumLock or the current keyboard layout.
        let physical = match event.physical_key {
            PhysicalKey::Code(WinitKeyCode::Numpad1) => Some(KeyCode::Numpad1),
            PhysicalKey::Code(WinitKeyCode::Numpad2) => Some(KeyCode::Numpad2),
            PhysicalKey::Code(WinitKeyCode::Numpad3) => Some(KeyCode::Numpad3),
            PhysicalKey::Code(WinitKeyCode::Numpad4) => Some(KeyCode::Numpad4),
            PhysicalKey::Code(WinitKeyCode::Numpad5) => Some(KeyCode::Numpad5),
            PhysicalKey::Code(WinitKeyCode::Numpad6) => Some(KeyCode::Numpad6),
            PhysicalKey::Code(WinitKeyCode::Numpad7) => Some(KeyCode::Numpad7),
            PhysicalKey::Code(WinitKeyCode::Numpad8) => Some(KeyCode::Numpad8),
            PhysicalKey::Code(WinitKeyCode::Numpad9) => Some(KeyCode::Numpad9),
            PhysicalKey::Code(WinitKeyCode::NumpadSubtract) => Some(KeyCode::NumpadSubtract),
            _ => None,
        };

        let key = if let Some(key) = physical {
            key
        } else {
            match &event.logical_key {
                Key::Named(named) => match named {
                    NamedKey::F5 => KeyCode::F5,
                    NamedKey::F10 => KeyCode::F10,
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
                    NamedKey::Insert => KeyCode::Insert,
                    NamedKey::Home => KeyCode::Home,
                    NamedKey::End => KeyCode::End,
                    NamedKey::PageUp => KeyCode::PageUp,
                    NamedKey::PageDown => KeyCode::PageDown,
                    _ => return None,
                },
                Key::Character(text) => text.chars().next().map(|c| {
                    // Shift is carried by Modifiers.  Normalizing the character
                    // prevents Ctrl+Maj+Z from relying on casing.
                    KeyCode::Character(c.to_ascii_lowercase())
                })?,
                _ => return None,
            }
        };

        Some(Self {
            key,
            physical_key: match event.physical_key {
                PhysicalKey::Code(code) => Some(code),
                PhysicalKey::Unidentified(_) => None,
            },
            location: event.location,
            modifiers,
            pressed: event.state.is_pressed(),
            repeat: event.repeat,
            window,
        })
    }
}
