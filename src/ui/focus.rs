//! Keyboard focus and semantic metadata shared by widgets and narration.

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FocusId(pub String);

impl FocusId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessibleRole {
    Button,
    MenuButton,
    MenuItem,
    TextField,
    List,
    ListItem,
    Checkbox,
    Slider,
    Dialog,
    Region,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessibleNode {
    pub id: FocusId,
    pub role: AccessibleRole,
    pub label: String,
    pub value: Option<String>,
    pub enabled: bool,
}

impl AccessibleNode {
    pub fn focusable(
        id: impl Into<String>,
        role: AccessibleRole,
        label: impl Into<String>,
    ) -> Self {
        Self {
            id: FocusId::new(id),
            role,
            label: label.into(),
            value: None,
            enabled: true,
        }
    }
}

#[derive(Debug, Clone)]
struct FocusScope {
    id: String,
    nodes: Vec<AccessibleNode>,
    focused: Option<usize>,
    restore: Option<FocusId>,
}

/// A stack is used so dialogs and popups trap focus and restore it reliably.
#[derive(Debug, Clone, Default)]
pub struct FocusManager {
    scopes: Vec<FocusScope>,
}

impl FocusManager {
    pub fn replace_root(&mut self, nodes: Vec<AccessibleNode>) {
        let previous = self.current_id().cloned();
        let focused = previous
            .as_ref()
            .and_then(|id| nodes.iter().position(|node| &node.id == id && node.enabled));
        let root = FocusScope {
            id: "root".into(),
            nodes,
            focused,
            restore: None,
        };
        if self.scopes.is_empty() {
            self.scopes.push(root);
        } else {
            self.scopes[0] = root;
        }
    }

    pub fn push_scope(&mut self, id: impl Into<String>, nodes: Vec<AccessibleNode>) {
        let restore = self.current_id().cloned();
        let focused = nodes.iter().position(|node| node.enabled);
        self.scopes.push(FocusScope {
            id: id.into(),
            nodes,
            focused,
            restore,
        });
    }

    pub fn pop_scope(&mut self) -> Option<FocusId> {
        if self.scopes.len() <= 1 {
            return None;
        }
        let restore = self.scopes.pop()?.restore;
        if let Some(id) = &restore {
            self.focus(id);
        }
        restore
    }

    pub fn active_scope_id(&self) -> Option<&str> {
        self.scopes.last().map(|scope| scope.id.as_str())
    }

    pub fn current(&self) -> Option<&AccessibleNode> {
        let scope = self.scopes.last()?;
        scope.focused.and_then(|index| scope.nodes.get(index))
    }

    pub fn current_id(&self) -> Option<&FocusId> {
        self.current().map(|node| &node.id)
    }

    pub fn focus(&mut self, id: &FocusId) -> bool {
        let Some(scope) = self.scopes.last_mut() else {
            return false;
        };
        let Some(index) = scope
            .nodes
            .iter()
            .position(|node| &node.id == id && node.enabled)
        else {
            return false;
        };
        scope.focused = Some(index);
        true
    }

    pub fn clear(&mut self) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.focused = None;
        }
    }

    pub fn focus_next(&mut self) -> Option<&AccessibleNode> {
        self.advance(1)
    }

    pub fn focus_previous(&mut self) -> Option<&AccessibleNode> {
        self.advance(-1)
    }

    fn advance(&mut self, direction: i32) -> Option<&AccessibleNode> {
        let scope = self.scopes.last_mut()?;
        if scope.nodes.is_empty() || !scope.nodes.iter().any(|node| node.enabled) {
            scope.focused = None;
            return None;
        }
        let len = scope.nodes.len() as i32;
        if scope.focused.is_none() {
            let index = if direction > 0 {
                scope.nodes.iter().position(|node| node.enabled)
            } else {
                scope.nodes.iter().rposition(|node| node.enabled)
            }?;
            scope.focused = Some(index);
            return scope.nodes.get(index);
        }
        let mut index = scope.focused.unwrap_or(0) as i32;
        for _ in 0..scope.nodes.len() {
            index = (index + direction).rem_euclid(len);
            if scope.nodes[index as usize].enabled {
                scope.focused = Some(index as usize);
                return scope.nodes.get(index as usize);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, enabled: bool) -> AccessibleNode {
        AccessibleNode {
            id: FocusId::new(id),
            role: AccessibleRole::Button,
            label: id.into(),
            value: None,
            enabled,
        }
    }

    #[test]
    fn traversal_wraps_and_skips_disabled_nodes() {
        let mut focus = FocusManager::default();
        focus.replace_root(vec![node("a", true), node("b", false), node("c", true)]);
        assert_eq!(focus.current_id(), None);
        assert_eq!(
            focus.focus_next().map(|node| node.id.clone()),
            Some(FocusId::new("a"))
        );
        assert_eq!(
            focus.focus_next().map(|node| node.id.clone()),
            Some(FocusId::new("c"))
        );
        assert_eq!(
            focus.focus_next().map(|node| node.id.clone()),
            Some(FocusId::new("a"))
        );
        assert_eq!(
            focus.focus_previous().map(|node| node.id.clone()),
            Some(FocusId::new("c"))
        );
    }

    #[test]
    fn nested_scope_traps_then_restores_focus() {
        let mut focus = FocusManager::default();
        focus.replace_root(vec![node("a", true), node("b", true)]);
        focus.focus_next();
        focus.focus_next();
        focus.push_scope("dialog", vec![node("cancel", true), node("ok", true)]);
        assert_eq!(focus.active_scope_id(), Some("dialog"));
        focus.focus_next();
        assert_eq!(focus.current_id(), Some(&FocusId::new("ok")));
        assert_eq!(focus.pop_scope(), Some(FocusId::new("b")));
        assert_eq!(focus.current_id(), Some(&FocusId::new("b")));
    }
}
