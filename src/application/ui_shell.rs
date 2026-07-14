//! Presentation state owned by the common application shell.

use crate::ui::Ui;

pub struct UiShell {
    pub ui: Ui,
}

impl UiShell {
    pub fn new(ui: Ui) -> Self {
        Self { ui }
    }
}
