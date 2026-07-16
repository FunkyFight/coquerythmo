//! Lightweight, deterministic automation graph for arranging rythmo lines.
//!
//! Execution and data are deliberately separate, as in a Blueprint graph.
//! Execution pins define order while `Line` connections carry the line being
//! processed by an entry node.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::constants::JS_MAX_SAFE_INTEGER;
use crate::project::Project;
use crate::rythmo_layout;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationNodeKind {
    ForEachLine,
    IfRole,
    SetTrack { track: u8 },
    LineReroute,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AutomationNode {
    pub id: u64,
    pub kind: AutomationNodeKind,
    /// Position relative to the top-left of the automation canvas.
    pub x: f32,
    pub y: f32,
    /// Role outputs explicitly enabled on an `IfRole` node.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roles: Vec<String>,
    /// Entry execution can be paused without deleting its wiring.
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum AutomationBranch {
    Next,
    Role(String),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationEdgeKind {
    #[default]
    Execution,
    Line,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutomationEdge {
    pub from_node: u64,
    #[serde(default)]
    pub kind: AutomationEdgeKind,
    pub branch: AutomationBranch,
    pub to_node: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AutomationGraph {
    #[serde(default)]
    pub nodes: Vec<AutomationNode>,
    #[serde(default)]
    pub edges: Vec<AutomationEdge>,
}

impl AutomationGraph {
    pub fn node(&self, id: u64) -> Option<&AutomationNode> {
        self.nodes.iter().find(|node| node.id == id)
    }

    pub fn add_node(&mut self, kind: AutomationNodeKind, x: f32, y: f32) -> Option<u64> {
        if matches!(kind, AutomationNodeKind::ForEachLine)
            && self
                .nodes
                .iter()
                .any(|node| matches!(node.kind, AutomationNodeKind::ForEachLine))
        {
            return None;
        }
        let id = loop {
            let candidate = rand::random::<u64>() % JS_MAX_SAFE_INTEGER;
            if candidate != 0 && self.node(candidate).is_none() {
                break candidate;
            }
        };
        self.nodes.push(AutomationNode {
            id,
            kind,
            x: x.max(0.0),
            y: y.max(0.0),
            roles: Vec::new(),
            enabled: true,
        });
        Some(id)
    }

    pub fn move_node(&mut self, id: u64, x: f32, y: f32) -> bool {
        let Some(node) = self.nodes.iter_mut().find(|node| node.id == id) else {
            return false;
        };
        let position = (x.max(0.0), y.max(0.0));
        if (node.x, node.y) == position {
            return false;
        }
        node.x = position.0;
        node.y = position.1;
        true
    }

    pub fn delete_node(&mut self, id: u64) -> bool {
        let old_len = self.nodes.len();
        self.nodes.retain(|node| node.id != id);
        self.edges
            .retain(|edge| edge.from_node != id && edge.to_node != id);
        self.nodes.len() != old_len
    }

    pub fn set_track(&mut self, id: u64, track: u8) -> bool {
        let Some(node) = self.nodes.iter_mut().find(|node| node.id == id) else {
            return false;
        };
        let AutomationNodeKind::SetTrack { track: current } = &mut node.kind else {
            return false;
        };
        let track = track.min(rythmo_layout::track_count().saturating_sub(1) as u8);
        if *current == track {
            return false;
        }
        *current = track;
        true
    }

    pub fn set_enabled(&mut self, id: u64, enabled: bool) -> bool {
        let Some(node) = self.nodes.iter_mut().find(|node| node.id == id) else {
            return false;
        };
        if !matches!(node.kind, AutomationNodeKind::ForEachLine) || node.enabled == enabled {
            return false;
        }
        node.enabled = enabled;
        true
    }

    pub fn add_role(&mut self, id: u64, role: String) -> bool {
        let role = role.trim().to_string();
        if role.is_empty() {
            return false;
        }
        let Some(node) = self.nodes.iter_mut().find(|node| node.id == id) else {
            return false;
        };
        if !matches!(node.kind, AutomationNodeKind::IfRole)
            || node.roles.iter().any(|existing| existing == &role)
        {
            return false;
        }
        node.roles.push(role);
        true
    }

    pub fn remove_role(&mut self, id: u64, role: &str) -> bool {
        let Some(node) = self.nodes.iter_mut().find(|node| node.id == id) else {
            return false;
        };
        if !matches!(node.kind, AutomationNodeKind::IfRole) {
            return false;
        }
        let old_len = node.roles.len();
        node.roles.retain(|existing| existing != role);
        let removed_from_node = node.roles.len() != old_len;
        let branch = AutomationBranch::Role(role.to_string());
        let old_edge_len = self.edges.len();
        self.edges.retain(|edge| {
            edge.kind != AutomationEdgeKind::Execution
                || edge.from_node != id
                || edge.branch != branch
        });
        removed_from_node || self.edges.len() != old_edge_len
    }

    pub fn connect(&mut self, edge: AutomationEdge) -> bool {
        if edge.from_node == edge.to_node
            || self.node(edge.from_node).is_none()
            || self.node(edge.to_node).is_none()
            || self
                .node(edge.to_node)
                .is_some_and(|node| matches!(node.kind, AutomationNodeKind::ForEachLine))
        {
            return false;
        }

        let Some(source) = self.node(edge.from_node) else {
            return false;
        };
        let valid_output = match (&edge.kind, &edge.branch, &source.kind) {
            (
                AutomationEdgeKind::Execution,
                AutomationBranch::Next,
                AutomationNodeKind::ForEachLine,
            ) => true,
            (
                AutomationEdgeKind::Execution,
                AutomationBranch::Role(role),
                AutomationNodeKind::IfRole,
            ) => source.roles.iter().any(|selected| selected == role),
            (
                AutomationEdgeKind::Line,
                AutomationBranch::Next,
                AutomationNodeKind::ForEachLine | AutomationNodeKind::LineReroute,
            ) => true,
            _ => false,
        };
        let Some(target) = self.node(edge.to_node) else {
            return false;
        };
        let valid_input = matches!(
            (&edge.kind, &target.kind),
            (
                AutomationEdgeKind::Execution,
                AutomationNodeKind::IfRole | AutomationNodeKind::SetTrack { .. },
            ) | (
                AutomationEdgeKind::Line,
                AutomationNodeKind::IfRole
                    | AutomationNodeKind::SetTrack { .. }
                    | AutomationNodeKind::LineReroute,
            )
        );
        if !valid_output || !valid_input {
            return false;
        }

        // Both inputs are single-link. Execution outputs are single-link too;
        // the Line data output may fan out to any number of consumers.
        let previous_edges = self.edges.clone();
        self.edges.retain(|existing| {
            !(existing.kind == edge.kind && existing.to_node == edge.to_node)
                && !(edge.kind == AutomationEdgeKind::Execution
                    && existing.kind == AutomationEdgeKind::Execution
                    && existing.from_node == edge.from_node
                    && existing.branch == edge.branch)
        });
        if self.would_create_cycle(edge.kind, edge.from_node, edge.to_node) {
            self.edges = previous_edges;
            return false;
        }
        self.edges.push(edge);
        true
    }

    pub fn disconnect(
        &mut self,
        from_node: u64,
        kind: AutomationEdgeKind,
        branch: &AutomationBranch,
    ) -> bool {
        let old_len = self.edges.len();
        self.edges.retain(|edge| {
            edge.from_node != from_node || edge.kind != kind || &edge.branch != branch
        });
        self.edges.len() != old_len
    }

    fn would_create_cycle(&self, kind: AutomationEdgeKind, from_node: u64, to_node: u64) -> bool {
        let mut stack = vec![to_node];
        let mut visited = HashSet::new();
        while let Some(node) = stack.pop() {
            if node == from_node {
                return true;
            }
            if visited.insert(node) {
                stack.extend(
                    self.edges
                        .iter()
                        .filter(|edge| edge.kind == kind && edge.from_node == node)
                        .map(|edge| edge.to_node),
                );
            }
        }
        false
    }

    /// Returns the minimal set of moves needed to make the project match the
    /// graph. The caller applies them through the normal edit boundary.
    pub fn desired_track_moves(&self, project: &Project) -> Vec<(u64, i64, f32)> {
        let nodes: HashMap<u64, &AutomationNode> =
            self.nodes.iter().map(|node| (node.id, node)).collect();
        let outgoing: HashMap<(u64, AutomationBranch), u64> = self
            .edges
            .iter()
            .filter(|edge| edge.kind == AutomationEdgeKind::Execution)
            .map(|edge| ((edge.from_node, edge.branch.clone()), edge.to_node))
            .collect();
        let line_sources: HashMap<u64, u64> = self
            .edges
            .iter()
            .filter(|edge| edge.kind == AutomationEdgeKind::Line)
            .map(|edge| (edge.to_node, edge.from_node))
            .collect();
        let entries: Vec<u64> = self
            .nodes
            .iter()
            .filter(|node| node.enabled && matches!(node.kind, AutomationNodeKind::ForEachLine))
            .map(|node| node.id)
            .collect();

        let mut moves = Vec::new();
        for line in project.lines() {
            let mut requested_track = None;
            for entry in &entries {
                let Some(mut next) = outgoing.get(&(*entry, AutomationBranch::Next)).copied()
                else {
                    continue;
                };
                let mut visited = HashSet::new();
                while visited.insert(next) {
                    let Some(node) = nodes.get(&next) else {
                        break;
                    };
                    if !line_reaches_entry(node.id, *entry, &nodes, &line_sources) {
                        break;
                    }
                    match &node.kind {
                        AutomationNodeKind::ForEachLine => break,
                        AutomationNodeKind::IfRole => {
                            let branch = AutomationBranch::Role(line.character_name.clone());
                            let Some(target) = outgoing.get(&(node.id, branch)).copied() else {
                                break;
                            };
                            next = target;
                        }
                        AutomationNodeKind::SetTrack { track } => {
                            requested_track =
                                Some(rythmo_layout::y_slot_for_track_index(*track as usize));
                            break;
                        }
                        AutomationNodeKind::LineReroute => break,
                    }
                }
            }
            if let Some(y_slot) = requested_track {
                if (line.y_slot - y_slot).abs() > f32::EPSILON {
                    moves.push((line.id, line.start_frame, y_slot));
                }
            }
        }
        moves
    }
}

fn line_reaches_entry(
    target: u64,
    entry: u64,
    nodes: &HashMap<u64, &AutomationNode>,
    line_sources: &HashMap<u64, u64>,
) -> bool {
    let mut current = target;
    let mut visited = HashSet::new();
    while visited.insert(current) {
        let Some(source) = line_sources.get(&current).copied() else {
            return false;
        };
        if source == entry {
            return true;
        }
        if !nodes
            .get(&source)
            .is_some_and(|node| matches!(node.kind, AutomationNodeKind::LineReroute))
        {
            return false;
        }
        current = source;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track_slot(index: usize) -> f32 {
        rythmo_layout::y_slot_for_track_index(index)
    }

    fn graph_for_role(role: &str, track: u8) -> AutomationGraph {
        let mut graph = AutomationGraph::default();
        let entry = graph
            .add_node(AutomationNodeKind::ForEachLine, 0.0, 0.0)
            .unwrap();
        let condition = graph
            .add_node(AutomationNodeKind::IfRole, 200.0, 0.0)
            .unwrap();
        assert!(graph.add_role(condition, role.into()));
        let set_track = graph
            .add_node(AutomationNodeKind::SetTrack { track }, 400.0, 0.0)
            .unwrap();
        assert!(graph.connect(AutomationEdge {
            from_node: entry,
            kind: AutomationEdgeKind::Execution,
            branch: AutomationBranch::Next,
            to_node: condition,
        }));
        assert!(graph.connect(AutomationEdge {
            from_node: condition,
            kind: AutomationEdgeKind::Execution,
            branch: AutomationBranch::Role(role.into()),
            to_node: set_track,
        }));
        for to_node in [condition, set_track] {
            assert!(graph.connect(AutomationEdge {
                from_node: entry,
                kind: AutomationEdgeKind::Line,
                branch: AutomationBranch::Next,
                to_node,
            }));
        }
        graph
    }

    #[test]
    fn role_branch_moves_only_matching_lines() {
        let mut project = Project::new();
        let alice =
            project.add_line_full(0, 24, track_slot(0), "A".into(), "Alice".into(), [1.0; 4]);
        let bob = project.add_line_full(30, 24, track_slot(1), "B".into(), "Bob".into(), [1.0; 4]);
        let moves = graph_for_role("Alice", 3).desired_track_moves(&project);
        assert_eq!(moves, vec![(alice, 0, track_slot(3))]);
        assert!(!moves.iter().any(|(id, _, _)| *id == bob));
    }

    #[test]
    fn track_number_is_used_as_a_zero_based_code_index() {
        let mut project = Project::new();
        let alice =
            project.add_line_full(0, 24, track_slot(0), "A".into(), "Alice".into(), [1.0; 4]);

        let moves = graph_for_role("Alice", 1).desired_track_moves(&project);

        assert_eq!(moves, vec![(alice, 0, track_slot(1))]);
    }

    #[test]
    fn track_zero_maps_to_the_first_visible_track() {
        let mut project = Project::new();
        let alice =
            project.add_line_full(0, 24, track_slot(1), "A".into(), "Alice".into(), [1.0; 4]);

        let moves = graph_for_role("Alice", 0).desired_track_moves(&project);

        assert_eq!(moves, vec![(alice, 0, track_slot(0))]);
        assert_eq!(track_slot(0), 0.0);
    }

    #[test]
    fn disabled_entry_pauses_execution_without_removing_wiring() {
        let mut project = Project::new();
        project.add_line_full(0, 24, track_slot(0), "A".into(), "Alice".into(), [1.0; 4]);
        let mut graph = graph_for_role("Alice", 2);
        let entry = graph
            .nodes
            .iter()
            .find(|node| matches!(node.kind, AutomationNodeKind::ForEachLine))
            .unwrap()
            .id;
        let edge_count = graph.edges.len();

        assert!(graph.set_enabled(entry, false));
        assert!(graph.desired_track_moves(&project).is_empty());
        assert_eq!(graph.edges.len(), edge_count);
        assert!(!graph.set_enabled(entry, false));
        assert!(graph.set_enabled(entry, true));
        assert!(!graph.desired_track_moves(&project).is_empty());
    }

    #[test]
    fn line_reroute_preserves_entry_line_provenance() {
        let mut project = Project::new();
        let alice =
            project.add_line_full(0, 24, track_slot(0), "A".into(), "Alice".into(), [1.0; 4]);
        let mut graph = graph_for_role("Alice", 2);
        let entry = graph
            .nodes
            .iter()
            .find(|node| matches!(node.kind, AutomationNodeKind::ForEachLine))
            .unwrap()
            .id;
        let consumers: Vec<u64> = graph
            .edges
            .iter()
            .filter(|edge| edge.kind == AutomationEdgeKind::Line)
            .map(|edge| edge.to_node)
            .collect();
        graph
            .edges
            .retain(|edge| edge.kind != AutomationEdgeKind::Line);
        let reroute = graph
            .add_node(AutomationNodeKind::LineReroute, 300.0, 180.0)
            .unwrap();
        assert!(graph.connect(AutomationEdge {
            from_node: entry,
            kind: AutomationEdgeKind::Line,
            branch: AutomationBranch::Next,
            to_node: reroute,
        }));
        for consumer in consumers {
            assert!(graph.connect(AutomationEdge {
                from_node: reroute,
                kind: AutomationEdgeKind::Line,
                branch: AutomationBranch::Next,
                to_node: consumer,
            }));
        }

        assert_eq!(
            graph.desired_track_moves(&project),
            vec![(alice, 0, track_slot(2))]
        );
    }

    #[test]
    fn execution_graph_rejects_cycles_and_duplicate_entries() {
        let mut graph = AutomationGraph::default();
        let entry = graph
            .add_node(AutomationNodeKind::ForEachLine, 0.0, 0.0)
            .unwrap();
        assert!(graph
            .add_node(AutomationNodeKind::ForEachLine, 0.0, 0.0)
            .is_none());
        let condition = graph
            .add_node(AutomationNodeKind::IfRole, 0.0, 0.0)
            .unwrap();
        assert!(graph.connect(AutomationEdge {
            from_node: entry,
            kind: AutomationEdgeKind::Execution,
            branch: AutomationBranch::Next,
            to_node: condition,
        }));
        assert!(!graph.connect(AutomationEdge {
            from_node: condition,
            kind: AutomationEdgeKind::Execution,
            branch: AutomationBranch::Role("Alice".into()),
            to_node: entry,
        }));
    }

    #[test]
    fn execution_without_line_data_does_not_run_the_node() {
        let mut project = Project::new();
        project.add_line_full(0, 24, track_slot(0), "A".into(), "Alice".into(), [1.0; 4]);
        let mut graph = graph_for_role("Alice", 3);
        graph
            .edges
            .retain(|edge| edge.kind != AutomationEdgeKind::Line);

        assert!(graph.desired_track_moves(&project).is_empty());
    }

    #[test]
    fn if_role_exposes_only_explicitly_added_roles() {
        let mut graph = AutomationGraph::default();
        let condition = graph
            .add_node(AutomationNodeKind::IfRole, 0.0, 0.0)
            .unwrap();
        assert!(graph.node(condition).unwrap().roles.is_empty());

        assert!(graph.add_role(condition, "Alice".into()));
        assert!(!graph.add_role(condition, "Alice".into()));
        assert_eq!(graph.node(condition).unwrap().roles, ["Alice"]);
        assert!(graph.remove_role(condition, "Alice"));
        assert!(graph.node(condition).unwrap().roles.is_empty());
    }

    #[test]
    fn graph_round_trips_with_project_settings() {
        let mut project = Project::new();
        let graph = graph_for_role("Alice", 3);
        let mut settings = project.settings().clone();
        settings.automation = graph.clone();
        project.set_settings(settings);

        let serialized =
            serde_json::to_string(&crate::export::ProjectData::from_project(&project, 24.0))
                .unwrap();
        let data: crate::export::ProjectData = serde_json::from_str(&serialized).unwrap();
        let mut restored = Project::new();
        data.apply_to_project(&mut restored, 24.0);

        assert_eq!(restored.settings().automation, graph);
    }
}
