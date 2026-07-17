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
    Home,
    End,
    PageUp,
    PageDown,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
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
    pub fn accessibility_label(&self) -> String {
        let mut parts = Vec::new();
        if self.modifiers.ctrl {
            parts.push(crate::i18n::t("shortcut.ctrl").to_string());
        }
        if self.modifiers.shift {
            parts.push(crate::i18n::t("shortcut.shift").to_string());
        }
        if self.modifiers.alt {
            parts.push(crate::i18n::t("shortcut.alt").to_string());
        }
        if self.modifiers.logo {
            parts.push(crate::i18n::t("shortcut.logo").to_string());
        }
        let key = match self.key {
            KeyCode::F5 => "F5".to_string(),
            KeyCode::F10 => "F10".to_string(),
            KeyCode::Escape => crate::i18n::t("shortcut.escape").to_string(),
            KeyCode::Space => crate::i18n::t("shortcut.space").to_string(),
            KeyCode::Tab => crate::i18n::t("shortcut.tab").to_string(),
            KeyCode::Delete => crate::i18n::t("shortcut.delete").to_string(),
            KeyCode::Backspace => crate::i18n::t("shortcut.backspace").to_string(),
            KeyCode::Enter => crate::i18n::t("shortcut.enter").to_string(),
            KeyCode::ArrowLeft => crate::i18n::t("shortcut.arrow_left").to_string(),
            KeyCode::ArrowRight => crate::i18n::t("shortcut.arrow_right").to_string(),
            KeyCode::ArrowUp => crate::i18n::t("shortcut.arrow_up").to_string(),
            KeyCode::ArrowDown => crate::i18n::t("shortcut.arrow_down").to_string(),
            KeyCode::Home => crate::i18n::t("shortcut.home").to_string(),
            KeyCode::End => crate::i18n::t("shortcut.end").to_string(),
            KeyCode::PageUp => crate::i18n::t("shortcut.page_up").to_string(),
            KeyCode::PageDown => crate::i18n::t("shortcut.page_down").to_string(),
            KeyCode::Digit1 => "1".to_string(),
            KeyCode::Digit2 => "2".to_string(),
            KeyCode::Digit3 => "3".to_string(),
            KeyCode::Digit4 => "4".to_string(),
            KeyCode::Numpad1 => format!("{} 1", crate::i18n::t("shortcut.numpad")),
            KeyCode::Numpad2 => format!("{} 2", crate::i18n::t("shortcut.numpad")),
            KeyCode::Numpad3 => format!("{} 3", crate::i18n::t("shortcut.numpad")),
            KeyCode::Numpad4 => format!("{} 4", crate::i18n::t("shortcut.numpad")),
            KeyCode::Numpad5 => format!("{} 5", crate::i18n::t("shortcut.numpad")),
            KeyCode::Numpad6 => format!("{} 6", crate::i18n::t("shortcut.numpad")),
            KeyCode::Numpad7 => format!("{} 7", crate::i18n::t("shortcut.numpad")),
            KeyCode::Numpad8 => format!("{} 8", crate::i18n::t("shortcut.numpad")),
            KeyCode::Numpad9 => format!("{} 9", crate::i18n::t("shortcut.numpad")),
            KeyCode::NumpadSubtract => format!(
                "{} {}",
                crate::i18n::t("shortcut.numpad"),
                crate::i18n::t("shortcut.subtract")
            ),
            KeyCode::Character(character) => character.to_uppercase().to_string(),
        };
        parts.push(key);
        format!(
            "{} {}",
            crate::i18n::t("shortcut.prefix"),
            parts.join(" + ")
        )
    }

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
            PhysicalKey::Code(WinitKeyCode::Digit1) => Some(KeyCode::Digit1),
            PhysicalKey::Code(WinitKeyCode::Digit2) => Some(KeyCode::Digit2),
            PhysicalKey::Code(WinitKeyCode::Digit3) => Some(KeyCode::Digit3),
            PhysicalKey::Code(WinitKeyCode::Digit4) => Some(KeyCode::Digit4),
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
