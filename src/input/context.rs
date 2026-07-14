//! Ordered input contexts.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InputContext {
    MainWindow,
    SecondaryWindow,
    VideoLoaded,
    Modal,
    TextEditing,
    Studio,
    Workspace,
    Global,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InputContextStack {
    contexts: Vec<InputContext>,
}

impl InputContextStack {
    pub fn new(contexts: impl IntoIterator<Item = InputContext>) -> Self {
        Self {
            contexts: contexts.into_iter().collect(),
        }
    }

    pub fn push(&mut self, context: InputContext) {
        self.contexts.push(context);
    }

    pub fn iter(&self) -> impl Iterator<Item = &InputContext> {
        self.contexts.iter()
    }
}
