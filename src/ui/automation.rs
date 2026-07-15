//! Native node-canvas UI for line automations.

use crate::automation::{
    AutomationBranch, AutomationEdgeKind, AutomationGraph, AutomationNode, AutomationNodeKind,
};
use crate::i18n::t;
use crate::project::Project;

use super::context_menu;
use super::primitives::{
    EventResponse, HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiAction, UiEvent, VAlign,
};

const NODE_HEADER_H: f32 = 34.0;
const NODE_W: f32 = 250.0;
const SET_TRACK_NODE_W: f32 = 270.0;
const ROLE_ROW_H: f32 = 27.0;
const ROLE_START_Y: f32 = 98.0;
const PIN_SIZE: f32 = 20.0;
const PICKER_HEADER_H: f32 = 34.0;
const PICKER_ROW_H: f32 = 30.0;
const CONTEXT_MENU_W: f32 = 250.0;
const CONTEXT_MENU_ITEM_H: f32 = 34.0;
const SET_TRACK_NODE_H: f32 = 56.0;
const EXEC_COLOR: [f32; 4] = [0.96, 0.58, 0.24, 1.0];
const LINE_COLOR: [f32; 4] = [0.30, 0.72, 1.0, 1.0];

#[derive(Clone)]
struct NodeDrag {
    node_id: u64,
    grab_x: f32,
    grab_y: f32,
    x: f32,
    y: f32,
}

#[derive(Clone)]
struct CanvasPan {
    last_x: f32,
    last_y: f32,
}

#[derive(Clone)]
struct AutomationContextMenu {
    rect: Rect,
    world_x: f32,
    world_y: f32,
    hovered: Option<usize>,
    source: Option<PendingConnection>,
}

impl AutomationContextMenu {
    fn item_count(&self) -> usize {
        if self.source.is_some() {
            1
        } else {
            4
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PendingConnection {
    Execution {
        node_id: u64,
        branch: AutomationBranch,
    },
    Line {
        node_id: u64,
    },
}

#[derive(Default)]
pub struct AutomationEditor {
    open: bool,
    selected_node: Option<u64>,
    dragging: Option<NodeDrag>,
    pending_connection: Option<PendingConnection>,
    role_picker_node: Option<u64>,
    role_picker_scroll: usize,
    canvas_pan: (f32, f32),
    panning: Option<CanvasPan>,
    context_menu: Option<AutomationContextMenu>,
}

impl AutomationEditor {
    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn open(&mut self) {
        self.open = true;
    }

    pub fn close(&mut self) {
        self.open = false;
        self.dragging = None;
        self.pending_connection = None;
        self.role_picker_node = None;
        self.panning = None;
        self.context_menu = None;
    }

    pub fn take_selected_node_for_deletion(&mut self) -> Option<u64> {
        if !self.open {
            return None;
        }
        let node_id = self.selected_node.take()?;
        self.dragging = None;
        self.pending_connection = None;
        self.role_picker_node = None;
        self.context_menu = None;
        Some(node_id)
    }

    fn node_position(&self, node: &AutomationNode) -> (f32, f32) {
        self.dragging
            .as_ref()
            .filter(|drag| drag.node_id == node.id)
            .map(|drag| (drag.x, drag.y))
            .unwrap_or((node.x, node.y))
    }

    fn node_rect(&self, zone: &Rect, graph: &AutomationGraph, node: &AutomationNode) -> Rect {
        let (x, y) = self.node_position(node);
        let height = node_height(graph, node);
        Rect {
            x: zone.x + self.canvas_pan.0 + x,
            y: zone.y + self.canvas_pan.1 + y,
            width: node_width(node),
            height,
        }
    }

    pub fn handle_event(
        &mut self,
        event: &UiEvent,
        zone: &Rect,
        graph: &AutomationGraph,
        project: &Project,
    ) -> Option<EventResponse> {
        if !self.open {
            return None;
        }

        let content = content_rect(zone);

        if let UiEvent::Scroll { x, y, delta, .. } = event {
            if zone.contains(*x, *y) {
                if let Some(node) = self.role_picker_node.and_then(|id| graph.node(id)) {
                    let available = available_roles(graph, node, project);
                    let (_, max_rows) =
                        role_picker_rect(zone, self.node_rect(zone, graph, node), available.len());
                    let max_scroll = available.len().saturating_sub(max_rows);
                    if *delta < 0.0 {
                        self.role_picker_scroll = (self.role_picker_scroll + 1).min(max_scroll);
                    } else if *delta > 0.0 {
                        self.role_picker_scroll = self.role_picker_scroll.saturating_sub(1);
                    }
                    return Some(EventResponse::Consumed);
                }
            }
        }

        let delete_requested = matches!(event, UiEvent::Delete)
            || matches!(event, UiEvent::KeyInput { text } if text == "\x7f");
        if delete_requested {
            if let Some(node_id) = self.take_selected_node_for_deletion() {
                return Some(EventResponse::Action(UiAction::AutomationDeleteNode {
                    node_id,
                }));
            }
            return Some(EventResponse::Consumed);
        }

        if matches!(event, UiEvent::KeyInput { text } if text == "\x1b") {
            self.pending_connection = None;
            self.role_picker_node = None;
            self.context_menu = None;
            return Some(EventResponse::Consumed);
        }

        if let UiEvent::ContextMenu { x, y } = event {
            if content.contains(*x, *y) {
                let rect = automation_context_menu_rect(&content, *x, *y, 4);
                self.context_menu = Some(AutomationContextMenu {
                    rect,
                    world_x: (*x - zone.x - self.canvas_pan.0).max(0.0),
                    world_y: (*y - zone.y - self.canvas_pan.1).max(0.0),
                    hovered: None,
                    source: None,
                });
                self.pending_connection = None;
                self.role_picker_node = None;
                return Some(EventResponse::Consumed);
            }
            if zone.contains(*x, *y) {
                self.context_menu = None;
                return Some(EventResponse::Consumed);
            }
            return None;
        }

        match event {
            UiEvent::MouseMove { x, y } => {
                if let Some(menu) = &mut self.context_menu {
                    menu.hovered = context_menu_index(menu.rect, menu.item_count(), *x, *y);
                    return Some(EventResponse::Consumed);
                }
                if let Some(drag) = &mut self.dragging {
                    drag.x = (*x - zone.x - self.canvas_pan.0 - drag.grab_x).max(0.0);
                    drag.y = (*y - zone.y - self.canvas_pan.1 - drag.grab_y).max(0.0);
                    return Some(EventResponse::Consumed);
                }
                if let Some(pan) = &mut self.panning {
                    self.canvas_pan.0 += *x - pan.last_x;
                    self.canvas_pan.1 += *y - pan.last_y;
                    pan.last_x = *x;
                    pan.last_y = *y;
                    return Some(EventResponse::Consumed);
                }
                if self.pending_connection.is_some() {
                    return Some(EventResponse::Consumed);
                }
            }
            UiEvent::MouseRelease { x, y } => {
                if let Some(drag) = self.dragging.take() {
                    return Some(EventResponse::Action(UiAction::AutomationMoveNode {
                        node_id: drag.node_id,
                        x: drag.x,
                        y: drag.y,
                    }));
                }
                if let Some(pending) = self.pending_connection.take() {
                    if let Some((kind, branch, to_node)) =
                        connection_target(self, zone, graph, &pending, *x, *y)
                    {
                        let from_node = match pending {
                            PendingConnection::Execution { node_id, .. }
                            | PendingConnection::Line { node_id } => node_id,
                        };
                        return Some(EventResponse::Action(UiAction::AutomationConnect {
                            from_node,
                            kind,
                            branch,
                            to_node,
                        }));
                    }
                    if matches!(pending, PendingConnection::Line { .. })
                        && content.contains(*x, *y)
                        && !node_or_pin_at_point(self, zone, graph, *x, *y)
                    {
                        self.context_menu = Some(AutomationContextMenu {
                            rect: automation_context_menu_rect(&content, *x, *y, 1),
                            world_x: (*x - zone.x - self.canvas_pan.0).max(0.0),
                            world_y: (*y - zone.y - self.canvas_pan.1).max(0.0),
                            hovered: Some(0),
                            source: Some(pending),
                        });
                    }
                    return Some(EventResponse::Consumed);
                }
                if self.panning.take().is_some() {
                    return Some(EventResponse::Consumed);
                }
            }
            _ => {}
        }

        let (x, y) = match event {
            UiEvent::MousePress { x, y } => (*x, *y),
            UiEvent::DoubleClick { x, y } if zone.contains(*x, *y) => {
                return Some(EventResponse::Consumed)
            }
            _ => return None,
        };
        if !zone.contains(x, y) {
            return None;
        }

        if let Some(menu) = self.context_menu.take() {
            if let Some(index) = context_menu_index(menu.rect, menu.item_count(), x, y) {
                if let Some(PendingConnection::Line { node_id }) = menu.source {
                    return Some(EventResponse::Action(
                        UiAction::AutomationAddConnectedNode {
                            kind: AutomationNodeKind::LineReroute,
                            x: menu.world_x,
                            y: menu.world_y,
                            from_node: node_id,
                            edge_kind: AutomationEdgeKind::Line,
                            branch: AutomationBranch::Next,
                        },
                    ));
                }
                let kind = match index {
                    0 => AutomationNodeKind::ForEachLine,
                    1 => AutomationNodeKind::IfRole,
                    2 => AutomationNodeKind::SetTrack { track: 0 },
                    _ => AutomationNodeKind::LineReroute,
                };
                if matches!(kind, AutomationNodeKind::ForEachLine)
                    && graph
                        .nodes
                        .iter()
                        .any(|node| matches!(node.kind, AutomationNodeKind::ForEachLine))
                {
                    return Some(EventResponse::Consumed);
                }
                return Some(EventResponse::Action(UiAction::AutomationAddNode {
                    kind,
                    x: menu.world_x,
                    y: menu.world_y,
                }));
            }
            return Some(EventResponse::Consumed);
        }

        if close_rect(zone).contains(x, y) {
            return Some(EventResponse::Action(UiAction::CloseAutomation));
        }

        // The role picker is an overlay and therefore owns clicks first.
        if let Some(node) = self.role_picker_node.and_then(|id| graph.node(id)) {
            let available = available_roles(graph, node, project);
            let (picker, max_rows) =
                role_picker_rect(zone, self.node_rect(zone, graph, node), available.len());
            let max_scroll = available.len().saturating_sub(max_rows);
            self.role_picker_scroll = self.role_picker_scroll.min(max_scroll);
            if picker.contains(x, y) {
                let list_y = picker.y + PICKER_HEADER_H + 4.0;
                if y >= list_y {
                    let row = ((y - list_y) / PICKER_ROW_H) as usize;
                    if row < max_rows {
                        if let Some(role) = available.get(self.role_picker_scroll + row) {
                            return Some(EventResponse::Action(UiAction::AutomationAddRole {
                                node_id: node.id,
                                role: (*role).to_string(),
                            }));
                        }
                    }
                }
                return Some(EventResponse::Consumed);
            }
            self.role_picker_node = None;
        }

        // Pins and controls take precedence over dragging a card.
        for node in graph.nodes.iter().rev() {
            let rect = self.node_rect(zone, graph, node);

            if matches!(node.kind, AutomationNodeKind::ForEachLine)
                && entry_enabled_rect(rect).contains(x, y)
            {
                self.selected_node = Some(node.id);
                return Some(EventResponse::Action(UiAction::AutomationSetNodeEnabled {
                    node_id: node.id,
                    enabled: !node.enabled,
                }));
            }

            if exec_input_rect(rect, node).is_some_and(|pin| pin.contains(x, y)) {
                return Some(EventResponse::Consumed);
            }

            if line_input_rect(rect, node).is_some_and(|pin| pin.contains(x, y)) {
                return Some(EventResponse::Consumed);
            }

            for (branch, pin) in exec_output_pins(rect, graph, node) {
                if pin.contains(x, y) {
                    let pending = PendingConnection::Execution {
                        node_id: node.id,
                        branch: branch.clone(),
                    };
                    self.selected_node = Some(node.id);
                    self.pending_connection = Some(pending);
                    return Some(EventResponse::Consumed);
                }
            }

            if line_output_rect(rect, node).is_some_and(|pin| pin.contains(x, y)) {
                let pending = PendingConnection::Line { node_id: node.id };
                self.selected_node = Some(node.id);
                self.pending_connection = Some(pending);
                return Some(EventResponse::Consumed);
            }

            if matches!(node.kind, AutomationNodeKind::IfRole) {
                for (index, role) in selected_roles(graph, node).iter().enumerate() {
                    if remove_role_rect(rect, index).contains(x, y) {
                        if self.pending_connection.as_ref().is_some_and(|pending| {
                            matches!(pending, PendingConnection::Execution { node_id, branch: AutomationBranch::Role(selected) } if *node_id == node.id && selected == role)
                        }) {
                            self.pending_connection = None;
                        }
                        return Some(EventResponse::Action(UiAction::AutomationRemoveRole {
                            node_id: node.id,
                            role: (*role).to_string(),
                        }));
                    }
                }
                if add_role_rect(rect).contains(x, y) {
                    self.selected_node = Some(node.id);
                    self.role_picker_node = Some(node.id);
                    self.role_picker_scroll = 0;
                    return Some(EventResponse::Consumed);
                }
            }

            if let AutomationNodeKind::SetTrack { track } = node.kind {
                let (previous, next) = track_button_rects(rect);
                if previous.contains(x, y) {
                    let track = if track == 0 { 3 } else { track - 1 };
                    return Some(EventResponse::Action(UiAction::AutomationSetTrack {
                        node_id: node.id,
                        track,
                    }));
                }
                if next.contains(x, y) {
                    let track = if track >= 3 { 0 } else { track + 1 };
                    return Some(EventResponse::Action(UiAction::AutomationSetTrack {
                        node_id: node.id,
                        track,
                    }));
                }
            }
        }

        for node in graph.nodes.iter().rev() {
            let rect = self.node_rect(zone, graph, node);
            if rect.contains(x, y) {
                self.selected_node = Some(node.id);
                self.dragging = Some(NodeDrag {
                    node_id: node.id,
                    grab_x: x - rect.x,
                    grab_y: y - rect.y,
                    x: node.x,
                    y: node.y,
                });
                return Some(EventResponse::Consumed);
            }
        }

        self.selected_node = None;
        self.pending_connection = None;
        self.role_picker_node = None;
        if content.contains(x, y) {
            self.panning = Some(CanvasPan {
                last_x: x,
                last_y: y,
            });
        }
        Some(EventResponse::Consumed)
    }

    pub fn render<'a>(
        &'a self,
        zone: &Rect,
        graph: &'a AutomationGraph,
        project: &'a Project,
        cursor: (f32, f32),
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
    ) {
        if !self.open {
            return;
        }
        let content = content_rect(zone);
        let quad_start = quads.len();
        let label_start = labels.len();

        let grid = [0.105, 0.11, 0.135, 0.75];
        let mut x = content.x + (14.0 + self.canvas_pan.0).rem_euclid(28.0);
        while x < content.x + content.width {
            push_solid(quads, [x, content.y, 1.0, content.height], grid, 0.0);
            x += 28.0;
        }
        let mut y = content.y + (14.0 + self.canvas_pan.1).rem_euclid(28.0);
        while y < content.y + content.height {
            push_solid(quads, [content.x, y, content.width, 1.0], grid, 0.0);
            y += 28.0;
        }

        for edge in &graph.edges {
            let Some(from) = graph.node(edge.from_node) else {
                continue;
            };
            let Some(to) = graph.node(edge.to_node) else {
                continue;
            };
            let from_rect = self.node_rect(zone, graph, from);
            let to_rect = self.node_rect(zone, graph, to);
            let points = match edge.kind {
                AutomationEdgeKind::Execution => {
                    exec_output_center(from_rect, graph, from, &edge.branch)
                        .zip(exec_input_center(to_rect, to))
                }
                AutomationEdgeKind::Line => {
                    line_output_center(from_rect, from).zip(line_input_center(to_rect, to))
                }
            };
            if let Some((start, end)) = points {
                let color = match edge.kind {
                    AutomationEdgeKind::Execution => EXEC_COLOR,
                    AutomationEdgeKind::Line => LINE_COLOR,
                };
                push_connection_inside(quads, &content, start, end, color);
            }
        }

        if let Some(pending) = &self.pending_connection {
            let cursor = clamp_point_to_rect(cursor, &content, 3.0);
            let start = match pending {
                PendingConnection::Execution { node_id, branch } => {
                    graph.node(*node_id).and_then(|node| {
                        exec_output_center(self.node_rect(zone, graph, node), graph, node, branch)
                    })
                }
                PendingConnection::Line { node_id } => graph
                    .node(*node_id)
                    .and_then(|node| line_output_center(self.node_rect(zone, graph, node), node)),
            };
            if let Some(start) = start {
                let color = match pending {
                    PendingConnection::Execution { .. } => EXEC_COLOR,
                    PendingConnection::Line { .. } => LINE_COLOR,
                };
                push_connection_inside(quads, &content, start, cursor, color);
            }
        }

        if graph.nodes.is_empty() {
            labels.push(label(
                t("automation.empty"),
                content,
                HAlign::Center,
                17.0,
                [122, 128, 148],
            ));
        }

        for node in &graph.nodes {
            self.render_node(zone, graph, node, quads, labels);
        }

        if let Some(node) = self.role_picker_node.and_then(|id| graph.node(id)) {
            self.render_role_picker(zone, graph, node, project, quads, labels);
        }

        labels.push(label(
            t("automation.delete_hint"),
            Rect {
                x: zone.x + 12.0,
                y: zone.y + zone.height - 24.0,
                width: 390.0,
                height: 20.0,
            },
            HAlign::Left,
            15.0,
            [108, 114, 132],
        ));

        clip_added_to_zone(quads, labels, quad_start, label_start, &content);
        self.render_close_button(zone, labels);
        self.render_context_menu(graph, quads, labels);
    }

    fn render_close_button<'a>(&'a self, zone: &Rect, labels: &mut Vec<LabelInfo<'a>>) {
        let close = close_rect(zone);
        labels.push(label("X", close, HAlign::Center, 20.0, [232, 234, 242]));
    }

    fn render_node<'a>(
        &'a self,
        zone: &Rect,
        graph: &'a AutomationGraph,
        node: &'a AutomationNode,
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
    ) {
        let rect = self.node_rect(zone, graph, node);
        let selected = self.selected_node == Some(node.id);
        let border = if selected {
            [0.46, 0.67, 1.0, 1.0]
        } else {
            [0.28, 0.3, 0.38, 1.0]
        };
        push_quad(
            quads,
            [rect.x, rect.y, rect.width, rect.height],
            [0.115, 0.12, 0.15, 1.0],
            [0.075, 0.08, 0.105, 1.0],
            border,
            if selected { 2.0 } else { 1.0 },
            8.0,
        );

        let (title, header_top, header_bottom) = match node.kind {
            AutomationNodeKind::ForEachLine => (
                t("automation.entry"),
                [0.14, 0.38, 0.48, 1.0],
                [0.09, 0.25, 0.33, 1.0],
            ),
            AutomationNodeKind::IfRole => (
                t("automation.if_role"),
                [0.38, 0.24, 0.54, 1.0],
                [0.25, 0.14, 0.38, 1.0],
            ),
            AutomationNodeKind::SetTrack { .. } => (
                t("automation.set_track"),
                [0.48, 0.29, 0.14, 1.0],
                [0.34, 0.18, 0.08, 1.0],
            ),
            AutomationNodeKind::LineReroute => (
                t("automation.line_reroute"),
                [0.12, 0.34, 0.52, 1.0],
                [0.07, 0.22, 0.38, 1.0],
            ),
        };
        push_quad(
            quads,
            [rect.x, rect.y, rect.width, NODE_HEADER_H],
            header_top,
            header_bottom,
            [0.0; 4],
            0.0,
            8.0,
        );
        let title_rect = if matches!(node.kind, AutomationNodeKind::SetTrack { .. }) {
            Rect {
                x: rect.x + 18.0,
                y: rect.y,
                width: rect.width - 116.0,
                height: NODE_HEADER_H,
            }
        } else {
            Rect {
                x: rect.x + 9.0,
                y: rect.y,
                width: rect.width - 18.0,
                height: NODE_HEADER_H,
            }
        };
        labels.push(label(
            title,
            title_rect,
            HAlign::Left,
            12.0,
            [242, 244, 250],
        ));

        if let Some(pin) = exec_input_rect(rect, node) {
            render_exec_pin(quads, labels, pin);
        }
        if let Some(pin) = line_input_rect(rect, node) {
            push_pin(quads, pin, LINE_COLOR);
            labels.push(label(
                t("automation.line_input"),
                Rect {
                    x: rect.x + 13.0,
                    y: pin.y - 1.0,
                    width: 62.0,
                    height: pin.height + 2.0,
                },
                HAlign::Left,
                12.0,
                [142, 190, 230],
            ));
        }

        match &node.kind {
            AutomationNodeKind::ForEachLine => {
                let checkbox = entry_enabled_rect(rect);
                push_quad(
                    quads,
                    [checkbox.x, checkbox.y, checkbox.width, checkbox.height],
                    [0.10, 0.11, 0.14, 1.0],
                    [0.07, 0.08, 0.10, 1.0],
                    [0.42, 0.48, 0.58, 1.0],
                    1.0,
                    4.0,
                );
                if node.enabled {
                    push_quad(
                        quads,
                        [
                            checkbox.x + 4.0,
                            checkbox.y + 4.0,
                            checkbox.width - 8.0,
                            checkbox.height - 8.0,
                        ],
                        [0.32, 0.80, 0.58, 1.0],
                        [0.20, 0.62, 0.43, 1.0],
                        [0.0; 4],
                        0.0,
                        2.0,
                    );
                }
                labels.push(label(
                    t("automation.enabled"),
                    Rect {
                        x: checkbox.x + checkbox.width + 7.0,
                        y: checkbox.y - 1.0,
                        width: rect.width - 92.0,
                        height: checkbox.height + 2.0,
                    },
                    HAlign::Left,
                    12.0,
                    if node.enabled {
                        [194, 224, 211]
                    } else {
                        [138, 144, 158]
                    },
                ));
                if let Some(pin) = line_output_rect(rect, node) {
                    labels.push(label(
                        t("automation.line_input"),
                        Rect {
                            x: rect.x + rect.width - 76.0,
                            y: pin.y - 1.0,
                            width: 56.0,
                            height: PIN_SIZE + 2.0,
                        },
                        HAlign::Right,
                        12.0,
                        [142, 190, 230],
                    ));
                    push_pin(quads, pin, LINE_COLOR);
                }
            }
            AutomationNodeKind::IfRole => {
                let roles = selected_roles(graph, node);
                if roles.is_empty() {
                    labels.push(label(
                        t("automation.no_selected_roles"),
                        Rect {
                            x: rect.x + 12.0,
                            y: rect.y + ROLE_START_Y - 4.0,
                            width: rect.width - 24.0,
                            height: ROLE_ROW_H,
                        },
                        HAlign::Center,
                        12.0,
                        [132, 138, 156],
                    ));
                } else {
                    for (index, role) in roles.iter().enumerate() {
                        let pin = exec_role_pin_rect(rect, index);
                        labels.push(label(
                            role,
                            Rect {
                                x: rect.x + 38.0,
                                y: pin.y - 2.0,
                                width: rect.width - 70.0,
                                height: PIN_SIZE + 4.0,
                            },
                            HAlign::Right,
                            13.0,
                            [205, 208, 220],
                        ));
                        let remove = remove_role_rect(rect, index);
                        push_quad(
                            quads,
                            [remove.x, remove.y, remove.width, remove.height],
                            [0.22, 0.14, 0.17, 1.0],
                            [0.16, 0.09, 0.12, 1.0],
                            [0.48, 0.24, 0.29, 0.9],
                            1.0,
                            4.0,
                        );
                        labels.push(label("X", remove, HAlign::Center, 11.0, [231, 169, 178]));
                    }
                }
                let add = add_role_rect(rect);
                push_quad(
                    quads,
                    [add.x, add.y, add.width, add.height],
                    [0.19, 0.17, 0.25, 1.0],
                    [0.13, 0.11, 0.19, 1.0],
                    [0.42, 0.34, 0.56, 0.9],
                    1.0,
                    5.0,
                );
                labels.push(label(
                    t("automation.add_role"),
                    add,
                    HAlign::Center,
                    13.0,
                    [215, 203, 235],
                ));
            }
            AutomationNodeKind::SetTrack { track } => {
                let (previous, next) = track_button_rects(rect);
                for (button, text) in [(previous, "-"), (next, "+")] {
                    push_quad(
                        quads,
                        [button.x, button.y, button.width, button.height],
                        [0.19, 0.2, 0.25, 1.0],
                        [0.13, 0.14, 0.18, 1.0],
                        [0.31, 0.34, 0.43, 1.0],
                        1.0,
                        5.0,
                    );
                    labels.push(label(text, button, HAlign::Center, 15.0, [225, 228, 238]));
                }
                let values = ["0", "1", "2", "3"];
                labels.push(label(
                    values[(*track).min(3) as usize],
                    Rect {
                        x: previous.x + previous.width + 2.0,
                        y: previous.y,
                        width: (next.x - previous.x - previous.width - 4.0).max(20.0),
                        height: previous.height,
                    },
                    HAlign::Center,
                    17.0,
                    [244, 202, 132],
                ));
            }
            AutomationNodeKind::LineReroute => {
                if let Some(pin) = line_output_rect(rect, node) {
                    labels.push(label(
                        t("automation.line_input"),
                        Rect {
                            x: rect.x + rect.width - 76.0,
                            y: pin.y - 1.0,
                            width: 56.0,
                            height: PIN_SIZE + 2.0,
                        },
                        HAlign::Right,
                        12.0,
                        [142, 190, 230],
                    ));
                    push_pin(quads, pin, LINE_COLOR);
                }
            }
        }

        for (_, pin) in exec_output_pins(rect, graph, node) {
            render_exec_pin(quads, labels, pin);
        }
    }

    fn render_role_picker<'a>(
        &'a self,
        zone: &Rect,
        graph: &'a AutomationGraph,
        node: &'a AutomationNode,
        project: &'a Project,
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
    ) {
        let available = available_roles(graph, node, project);
        let (picker, max_rows) =
            role_picker_rect(zone, self.node_rect(zone, graph, node), available.len());
        let max_scroll = available.len().saturating_sub(max_rows);
        let scroll = self.role_picker_scroll.min(max_scroll);
        push_quad(
            quads,
            [picker.x, picker.y, picker.width, picker.height],
            [0.16, 0.16, 0.205, 1.0],
            [0.10, 0.10, 0.135, 1.0],
            [0.42, 0.43, 0.54, 1.0],
            1.0,
            7.0,
        );
        labels.push(label(
            t("automation.choose_role"),
            Rect {
                x: picker.x + 7.0,
                y: picker.y,
                width: picker.width - 14.0,
                height: PICKER_HEADER_H,
            },
            HAlign::Left,
            13.0,
            [224, 225, 235],
        ));

        if available.is_empty() {
            labels.push(label(
                t("automation.all_roles_added"),
                Rect {
                    x: picker.x + 7.0,
                    y: picker.y + PICKER_HEADER_H + 3.0,
                    width: picker.width - 14.0,
                    height: PICKER_ROW_H,
                },
                HAlign::Center,
                11.0,
                [135, 139, 155],
            ));
            return;
        }

        for (row, role) in available.iter().skip(scroll).take(max_rows).enumerate() {
            let rect = Rect {
                x: picker.x + 5.0,
                y: picker.y + PICKER_HEADER_H + 4.0 + row as f32 * PICKER_ROW_H,
                width: picker.width - 10.0,
                height: PICKER_ROW_H - 2.0,
            };
            push_quad(
                quads,
                [rect.x, rect.y, rect.width, rect.height],
                [0.21, 0.21, 0.27, 1.0],
                [0.15, 0.15, 0.20, 1.0],
                [0.32, 0.33, 0.42, 0.8],
                1.0,
                4.0,
            );
            labels.push(label(role, rect, HAlign::Left, 13.0, [215, 217, 228]));
        }
    }

    fn render_context_menu<'a>(
        &'a self,
        graph: &AutomationGraph,
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
    ) {
        let Some(menu) = &self.context_menu else {
            return;
        };
        context_menu::render_panel(quads, menu.rect);
        let has_entry = graph
            .nodes
            .iter()
            .any(|node| matches!(node.kind, AutomationNodeKind::ForEachLine));
        let items: Vec<&str> = if menu.source.is_some() {
            vec![t("automation.add_line_reroute")]
        } else {
            vec![
                t("automation.add_entry"),
                t("automation.add_if_role"),
                t("automation.add_set_track"),
                t("automation.add_line_reroute"),
            ]
        };
        for (index, text) in items.into_iter().enumerate() {
            let rect = Rect {
                x: menu.rect.x,
                y: menu.rect.y + index as f32 * CONTEXT_MENU_ITEM_H,
                width: menu.rect.width,
                height: CONTEXT_MENU_ITEM_H,
            };
            context_menu::render_item(
                quads,
                labels,
                rect,
                text,
                menu.hovered == Some(index) && !(index == 0 && has_entry),
                false,
                14.0,
            );
        }
    }
}

fn content_rect(zone: &Rect) -> Rect {
    *zone
}

fn automation_context_menu_rect(content: &Rect, x: f32, y: f32, item_count: usize) -> Rect {
    let height = CONTEXT_MENU_ITEM_H * item_count as f32;
    Rect {
        x: x.clamp(
            content.x + context_menu::MARGIN,
            (content.x + content.width - CONTEXT_MENU_W - context_menu::MARGIN)
                .max(content.x + context_menu::MARGIN),
        ),
        y: y.clamp(
            content.y + context_menu::MARGIN,
            (content.y + content.height - height - context_menu::MARGIN)
                .max(content.y + context_menu::MARGIN),
        ),
        width: CONTEXT_MENU_W,
        height,
    }
}

fn context_menu_index(rect: Rect, item_count: usize, x: f32, y: f32) -> Option<usize> {
    if !rect.contains(x, y) {
        return None;
    }
    let index = ((y - rect.y) / CONTEXT_MENU_ITEM_H).floor() as usize;
    (index < item_count).then_some(index)
}

fn connection_target(
    editor: &AutomationEditor,
    zone: &Rect,
    graph: &AutomationGraph,
    pending: &PendingConnection,
    x: f32,
    y: f32,
) -> Option<(AutomationEdgeKind, AutomationBranch, u64)> {
    for node in graph.nodes.iter().rev() {
        let from_node = match pending {
            PendingConnection::Execution { node_id, .. } | PendingConnection::Line { node_id } => {
                *node_id
            }
        };
        if from_node == node.id {
            continue;
        }
        let rect = editor.node_rect(zone, graph, node);
        match pending {
            PendingConnection::Execution { branch, .. }
                if exec_input_rect(rect, node).is_some_and(|pin| pin.contains(x, y)) =>
            {
                return Some((AutomationEdgeKind::Execution, branch.clone(), node.id));
            }
            PendingConnection::Line { .. }
                if line_input_rect(rect, node).is_some_and(|pin| pin.contains(x, y)) =>
            {
                return Some((AutomationEdgeKind::Line, AutomationBranch::Next, node.id));
            }
            _ => {}
        }
    }
    None
}

fn node_or_pin_at_point(
    editor: &AutomationEditor,
    zone: &Rect,
    graph: &AutomationGraph,
    x: f32,
    y: f32,
) -> bool {
    graph.nodes.iter().any(|node| {
        let rect = editor.node_rect(zone, graph, node);
        rect.contains(x, y)
            || exec_input_rect(rect, node).is_some_and(|pin| pin.contains(x, y))
            || line_input_rect(rect, node).is_some_and(|pin| pin.contains(x, y))
            || exec_output_pins(rect, graph, node)
                .into_iter()
                .any(|(_, pin)| pin.contains(x, y))
            || line_output_rect(rect, node).is_some_and(|pin| pin.contains(x, y))
    })
}

fn selected_roles<'a>(graph: &'a AutomationGraph, node: &'a AutomationNode) -> Vec<&'a str> {
    let mut roles: Vec<&str> = node.roles.iter().map(String::as_str).collect();
    for edge in graph
        .edges
        .iter()
        .filter(|edge| edge.kind == AutomationEdgeKind::Execution && edge.from_node == node.id)
    {
        if let AutomationBranch::Role(role) = &edge.branch {
            if !roles.contains(&role.as_str()) {
                roles.push(role);
            }
        }
    }
    roles
}

fn available_roles<'a>(
    graph: &AutomationGraph,
    node: &AutomationNode,
    project: &'a Project,
) -> Vec<&'a str> {
    let selected = selected_roles(graph, node);
    project
        .known_characters()
        .iter()
        .map(|character| character.name.as_str())
        .filter(|role| !selected.contains(role))
        .collect()
}

fn node_height(graph: &AutomationGraph, node: &AutomationNode) -> f32 {
    match node.kind {
        AutomationNodeKind::ForEachLine => 112.0,
        AutomationNodeKind::IfRole => {
            ROLE_START_Y + selected_roles(graph, node).len().max(1) as f32 * ROLE_ROW_H + 42.0
        }
        AutomationNodeKind::SetTrack { .. } => SET_TRACK_NODE_H,
        AutomationNodeKind::LineReroute => 76.0,
    }
}

fn node_width(node: &AutomationNode) -> f32 {
    if matches!(node.kind, AutomationNodeKind::SetTrack { .. }) {
        SET_TRACK_NODE_W
    } else {
        NODE_W
    }
}

fn close_rect(zone: &Rect) -> Rect {
    Rect {
        x: zone.x + zone.width - 42.0,
        y: zone.y + 8.0,
        width: 34.0,
        height: 34.0,
    }
}

fn entry_enabled_rect(rect: Rect) -> Rect {
    Rect {
        x: rect.x + 12.0,
        y: rect.y + NODE_HEADER_H + 7.0,
        width: 20.0,
        height: 20.0,
    }
}

fn exec_input_rect(rect: Rect, node: &AutomationNode) -> Option<Rect> {
    if matches!(
        node.kind,
        AutomationNodeKind::ForEachLine | AutomationNodeKind::LineReroute
    ) {
        return None;
    }
    Some(Rect {
        x: rect.x - PIN_SIZE / 2.0,
        y: rect.y
            + if matches!(node.kind, AutomationNodeKind::SetTrack { .. }) {
                3.0
            } else {
                NODE_HEADER_H + 7.0
            },
        width: PIN_SIZE,
        height: PIN_SIZE,
    })
}

fn line_input_rect(rect: Rect, node: &AutomationNode) -> Option<Rect> {
    if matches!(node.kind, AutomationNodeKind::ForEachLine) {
        return None;
    }
    Some(Rect {
        x: rect.x - PIN_SIZE / 2.0,
        y: rect.y
            + match node.kind {
                AutomationNodeKind::SetTrack { .. } => 32.0,
                AutomationNodeKind::LineReroute => NODE_HEADER_H + 7.0,
                _ => NODE_HEADER_H + 32.0,
            },
        width: PIN_SIZE,
        height: PIN_SIZE,
    })
}

fn exec_input_center(rect: Rect, node: &AutomationNode) -> Option<(f32, f32)> {
    exec_input_rect(rect, node).map(rect_center)
}

fn line_input_center(rect: Rect, node: &AutomationNode) -> Option<(f32, f32)> {
    line_input_rect(rect, node).map(rect_center)
}

fn exec_role_pin_rect(rect: Rect, index: usize) -> Rect {
    Rect {
        x: rect.x + rect.width - PIN_SIZE / 2.0,
        y: rect.y + ROLE_START_Y + index as f32 * ROLE_ROW_H,
        width: PIN_SIZE,
        height: PIN_SIZE,
    }
}

fn exec_output_pins(
    rect: Rect,
    graph: &AutomationGraph,
    node: &AutomationNode,
) -> Vec<(AutomationBranch, Rect)> {
    match node.kind {
        AutomationNodeKind::ForEachLine => vec![(
            AutomationBranch::Next,
            Rect {
                x: rect.x + rect.width - PIN_SIZE / 2.0,
                y: rect.y + NODE_HEADER_H + 7.0,
                width: PIN_SIZE,
                height: PIN_SIZE,
            },
        )],
        AutomationNodeKind::IfRole => selected_roles(graph, node)
            .into_iter()
            .enumerate()
            .map(|(index, role)| {
                (
                    AutomationBranch::Role(role.to_string()),
                    exec_role_pin_rect(rect, index),
                )
            })
            .collect(),
        AutomationNodeKind::SetTrack { .. } | AutomationNodeKind::LineReroute => Vec::new(),
    }
}

fn line_output_rect(rect: Rect, node: &AutomationNode) -> Option<Rect> {
    if !matches!(
        node.kind,
        AutomationNodeKind::ForEachLine | AutomationNodeKind::LineReroute
    ) {
        return None;
    }
    Some(Rect {
        x: rect.x + rect.width - PIN_SIZE / 2.0,
        y: rect.y
            + NODE_HEADER_H
            + if matches!(node.kind, AutomationNodeKind::LineReroute) {
                7.0
            } else {
                34.0
            },
        width: PIN_SIZE,
        height: PIN_SIZE,
    })
}

fn exec_output_center(
    rect: Rect,
    graph: &AutomationGraph,
    node: &AutomationNode,
    branch: &AutomationBranch,
) -> Option<(f32, f32)> {
    exec_output_pins(rect, graph, node)
        .into_iter()
        .find(|(candidate, _)| candidate == branch)
        .map(|(_, pin)| rect_center(pin))
}

fn line_output_center(rect: Rect, node: &AutomationNode) -> Option<(f32, f32)> {
    line_output_rect(rect, node).map(rect_center)
}

fn remove_role_rect(rect: Rect, index: usize) -> Rect {
    Rect {
        x: rect.x + 9.0,
        y: rect.y + ROLE_START_Y + index as f32 * ROLE_ROW_H + 1.0,
        width: 16.0,
        height: 16.0,
    }
}

fn add_role_rect(rect: Rect) -> Rect {
    Rect {
        x: rect.x + 10.0,
        y: rect.y + rect.height - 29.0,
        width: rect.width - 20.0,
        height: 23.0,
    }
}

fn track_button_rects(rect: Rect) -> (Rect, Rect) {
    let y = rect.y + 30.0;
    (
        Rect {
            x: rect.x + rect.width - 86.0,
            y,
            width: 22.0,
            height: 24.0,
        },
        Rect {
            x: rect.x + rect.width - 30.0,
            y,
            width: 22.0,
            height: 24.0,
        },
    )
}

fn role_picker_rect(zone: &Rect, node_rect: Rect, item_count: usize) -> (Rect, usize) {
    let content = content_rect(zone);
    let width = 230.0_f32.min((content.width - 8.0).max(80.0));
    let max_rows =
        (((content.height - PICKER_HEADER_H - 16.0) / PICKER_ROW_H).floor() as usize).clamp(1, 12);
    let visible_rows = item_count.max(1).min(max_rows);
    let height = PICKER_HEADER_H + 8.0 + visible_rows as f32 * PICKER_ROW_H;
    let preferred_x = node_rect.x + node_rect.width + 7.0;
    let x = preferred_x.clamp(
        content.x + 4.0,
        (content.x + content.width - width - 4.0).max(content.x + 4.0),
    );
    let y = (node_rect.y + NODE_HEADER_H).clamp(
        content.y + 4.0,
        (content.y + content.height - height - 4.0).max(content.y + 4.0),
    );
    (
        Rect {
            x,
            y,
            width,
            height,
        },
        max_rows,
    )
}

fn rect_center(rect: Rect) -> (f32, f32) {
    (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0)
}

fn clamp_point_to_rect(point: (f32, f32), rect: &Rect, margin: f32) -> (f32, f32) {
    (
        point.0.clamp(rect.x + margin, rect.x + rect.width - margin),
        point
            .1
            .clamp(rect.y + margin, rect.y + rect.height - margin),
    )
}

fn push_connection_inside(
    quads: &mut Vec<QuadInstance>,
    zone: &Rect,
    start: (f32, f32),
    end: (f32, f32),
    color: [f32; 4],
) {
    if !zone.contains(start.0, start.1) || !zone.contains(end.0, end.1) {
        return;
    }
    let dx = end.0 - start.0;
    let dy = end.1 - start.1;
    let len = (dx * dx + dy * dy).sqrt().max(1.0);
    quads.push(QuadInstance {
        rect: [
            (start.0 + end.0) / 2.0 - len / 2.0,
            (start.1 + end.1) / 2.0 - 1.5,
            len,
            3.0,
        ],
        color,
        color_bottom: color,
        border_color: [0.0; 4],
        border_width: 0.0,
        border_radius: 1.5,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0, 0.0, 0.0, 0.45],
        shadow_blur: 2.0,
        rotation: dy.atan2(dx),
        _padding: [0.0; 2],
    });
}

fn render_exec_pin<'a>(quads: &mut Vec<QuadInstance>, labels: &mut Vec<LabelInfo<'a>>, rect: Rect) {
    push_quad(
        quads,
        [rect.x, rect.y, rect.width, rect.height],
        EXEC_COLOR,
        [0.62, 0.28, 0.08, 1.0],
        [1.0, 0.78, 0.48, 1.0],
        1.0,
        5.0,
    );
    labels.push(label(">", rect, HAlign::Center, 12.0, [255, 244, 224]));
}

fn push_pin(quads: &mut Vec<QuadInstance>, rect: Rect, color: [f32; 4]) {
    push_quad(
        quads,
        [rect.x, rect.y, rect.width, rect.height],
        color,
        color,
        [0.72, 0.86, 1.0, 1.0],
        1.0,
        rect.width / 2.0,
    );
}

fn push_solid(quads: &mut Vec<QuadInstance>, rect: [f32; 4], color: [f32; 4], radius: f32) {
    push_quad(quads, rect, color, color, [0.0; 4], 0.0, radius);
}

fn push_quad(
    quads: &mut Vec<QuadInstance>,
    rect: [f32; 4],
    top: [f32; 4],
    bottom: [f32; 4],
    border: [f32; 4],
    border_width: f32,
    radius: f32,
) {
    quads.push(QuadInstance {
        rect,
        color: top,
        color_bottom: bottom,
        border_color: border,
        border_width,
        border_radius: radius,
        shadow_offset: [0.0, 2.0],
        shadow_color: [0.0, 0.0, 0.0, 0.38],
        shadow_blur: 6.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    });
}

fn clip_added_to_zone<'a>(
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    quad_start: usize,
    label_start: usize,
    zone: &Rect,
) {
    let added_quads = quads.split_off(quad_start);
    for mut quad in added_quads {
        if quad.rotation.abs() > f32::EPSILON {
            quads.push(quad);
            continue;
        }
        let Some(clipped) = intersect_rect(
            Rect {
                x: quad.rect[0],
                y: quad.rect[1],
                width: quad.rect[2],
                height: quad.rect[3],
            },
            *zone,
        ) else {
            continue;
        };
        quad.rect = [clipped.x, clipped.y, clipped.width, clipped.height];
        if clipped.x <= zone.x + 0.5
            || clipped.y <= zone.y + 0.5
            || clipped.x + clipped.width >= zone.x + zone.width - 0.5
            || clipped.y + clipped.height >= zone.y + zone.height - 0.5
        {
            quad.shadow_blur = 0.0;
            quad.shadow_color = [0.0; 4];
        }
        quads.push(quad);
    }

    let added_labels = labels.split_off(label_start);
    for mut info in added_labels {
        let Some(bounds) = intersect_rect(info.bounds, *zone) else {
            continue;
        };
        info.bounds = bounds;
        labels.push(info);
    }
}

fn intersect_rect(a: Rect, b: Rect) -> Option<Rect> {
    let left = a.x.max(b.x);
    let top = a.y.max(b.y);
    let right = (a.x + a.width).min(b.x + b.width);
    let bottom = (a.y + a.height).min(b.y + b.height);
    (right > left && bottom > top).then_some(Rect {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    })
}

fn label<'a>(
    text: &'a str,
    bounds: Rect,
    h_align: HAlign,
    size: f32,
    color: [u8; 3],
) -> LabelInfo<'a> {
    LabelInfo {
        text,
        bounds,
        h_align,
        v_align: VAlign::Center,
        overflow: Overflow::Ellipsis,
        padding: 2.0,
        font_size_override: Some(size),
        color_override: Some(color),
        font_family_override: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(rect: Rect) -> UiEvent {
        UiEvent::MousePress {
            x: rect.x + rect.width / 2.0,
            y: rect.y + rect.height / 2.0,
        }
    }

    fn release(rect: Rect) -> UiEvent {
        UiEvent::MouseRelease {
            x: rect.x + rect.width / 2.0,
            y: rect.y + rect.height / 2.0,
        }
    }

    fn entry_and_condition() -> (AutomationGraph, u64, u64) {
        let mut graph = AutomationGraph::default();
        let entry = graph
            .add_node(AutomationNodeKind::ForEachLine, 10.0, 10.0)
            .unwrap();
        let condition = graph
            .add_node(AutomationNodeKind::IfRole, 320.0, 10.0)
            .unwrap();
        (graph, entry, condition)
    }

    #[test]
    fn set_track_node_fits_within_two_grid_cells() {
        let mut graph = AutomationGraph::default();
        let node_id = graph
            .add_node(AutomationNodeKind::SetTrack { track: 0 }, 10.0, 10.0)
            .unwrap();
        let zone = Rect {
            x: 0.0,
            y: 32.0,
            width: 800.0,
            height: 400.0,
        };
        let editor = AutomationEditor::default();
        let node = graph.node(node_id).unwrap();
        let rect = editor.node_rect(&zone, &graph, node);

        assert_eq!(rect.height, 56.0);
        assert_eq!(rect.width, 270.0);
        for pin in [
            exec_input_rect(rect, node).unwrap(),
            line_input_rect(rect, node).unwrap(),
        ] {
            assert!(pin.y >= rect.y);
            assert!(pin.y + pin.height <= rect.y + rect.height);
        }
        let (previous, next) = track_button_rects(rect);
        let line_pin = line_input_rect(rect, node).unwrap();
        let line_center_y = line_pin.y + line_pin.height / 2.0;
        for button in [previous, next] {
            assert!(button.y >= rect.y);
            assert!(button.y + button.height <= rect.y + rect.height);
            assert_eq!(button.y + button.height / 2.0, line_center_y);
        }
    }

    #[test]
    fn execution_pins_do_not_render_redundant_execution_labels() {
        let (graph, _, _) = entry_and_condition();
        let project = Project::new();
        let zone = Rect {
            x: 0.0,
            y: 32.0,
            width: 800.0,
            height: 400.0,
        };
        let mut editor = AutomationEditor::default();
        editor.open();
        let mut quads = Vec::new();
        let mut labels = Vec::new();

        editor.render(&zone, &graph, &project, (0.0, 0.0), &mut quads, &mut labels);

        assert!(!labels
            .iter()
            .any(|label| label.text == t("automation.exec_input")));
    }

    #[test]
    fn entry_checkbox_toggles_execution() {
        let (graph, entry, _) = entry_and_condition();
        let project = Project::new();
        let zone = Rect {
            x: 0.0,
            y: 32.0,
            width: 800.0,
            height: 400.0,
        };
        let mut editor = AutomationEditor::default();
        editor.open();
        let node = graph.node(entry).unwrap();
        assert!(node.enabled);
        let rect = editor.node_rect(&zone, &graph, node);

        assert_eq!(
            editor.handle_event(&press(entry_enabled_rect(rect)), &zone, &graph, &project),
            Some(EventResponse::Action(UiAction::AutomationSetNodeEnabled {
                node_id: entry,
                enabled: false,
            }))
        );
    }

    #[test]
    fn line_output_connects_only_to_the_line_input() {
        let (graph, entry, condition) = entry_and_condition();
        let project = Project::new();
        let zone = Rect {
            x: 0.0,
            y: 32.0,
            width: 800.0,
            height: 360.0,
        };
        let mut editor = AutomationEditor::default();
        editor.open();
        let entry_node = graph.node(entry).unwrap();
        let condition_node = graph.node(condition).unwrap();
        let entry_rect = editor.node_rect(&zone, &graph, entry_node);
        let condition_rect = editor.node_rect(&zone, &graph, condition_node);

        assert_eq!(
            editor.handle_event(
                &press(line_output_rect(entry_rect, entry_node).unwrap()),
                &zone,
                &graph,
                &project,
            ),
            Some(EventResponse::Consumed)
        );
        assert_eq!(
            editor.handle_event(
                &release(exec_input_rect(condition_rect, condition_node).unwrap()),
                &zone,
                &graph,
                &project,
            ),
            Some(EventResponse::Consumed)
        );
        assert_eq!(
            editor.handle_event(
                &press(line_output_rect(entry_rect, entry_node).unwrap()),
                &zone,
                &graph,
                &project,
            ),
            Some(EventResponse::Consumed)
        );
        assert!(matches!(
            editor.handle_event(
                &release(line_input_rect(condition_rect, condition_node).unwrap()),
                &zone,
                &graph,
                &project,
            ),
            Some(EventResponse::Action(UiAction::AutomationConnect {
                kind: AutomationEdgeKind::Line,
                ..
            }))
        ));
    }

    #[test]
    fn empty_canvas_drag_pans_nodes() {
        let (graph, entry, _) = entry_and_condition();
        let project = Project::new();
        let zone = Rect {
            x: 0.0,
            y: 32.0,
            width: 900.0,
            height: 500.0,
        };
        let mut editor = AutomationEditor::default();
        editor.open();
        let before = editor.node_rect(&zone, &graph, graph.node(entry).unwrap());

        editor.handle_event(
            &UiEvent::MousePress { x: 850.0, y: 450.0 },
            &zone,
            &graph,
            &project,
        );
        editor.handle_event(
            &UiEvent::MouseMove { x: 810.0, y: 420.0 },
            &zone,
            &graph,
            &project,
        );
        editor.handle_event(
            &UiEvent::MouseRelease { x: 810.0, y: 420.0 },
            &zone,
            &graph,
            &project,
        );

        let after = editor.node_rect(&zone, &graph, graph.node(entry).unwrap());
        assert_eq!(after.x, before.x - 40.0);
        assert_eq!(after.y, before.y - 30.0);
    }

    #[test]
    fn right_click_menu_creates_a_node_at_the_canvas_position() {
        let graph = AutomationGraph::default();
        let project = Project::new();
        let zone = Rect {
            x: 0.0,
            y: 32.0,
            width: 800.0,
            height: 400.0,
        };
        let mut editor = AutomationEditor::default();
        editor.open();
        assert_eq!(
            editor.handle_event(
                &UiEvent::ContextMenu { x: 300.0, y: 180.0 },
                &zone,
                &graph,
                &project,
            ),
            Some(EventResponse::Consumed)
        );
        let menu = editor.context_menu.as_ref().unwrap().rect;
        assert!(matches!(
            editor.handle_event(
                &press(Rect {
                    height: CONTEXT_MENU_ITEM_H,
                    ..menu
                }),
                &zone,
                &graph,
                &project
            ),
            Some(EventResponse::Action(UiAction::AutomationAddNode {
                kind: AutomationNodeKind::ForEachLine,
                ..
            }))
        ));
    }

    #[test]
    fn dropping_a_line_cable_on_empty_canvas_offers_a_connected_reroute() {
        let mut graph = AutomationGraph::default();
        let entry = graph
            .add_node(AutomationNodeKind::ForEachLine, 10.0, 10.0)
            .unwrap();
        let project = Project::new();
        let zone = Rect {
            x: 0.0,
            y: 32.0,
            width: 800.0,
            height: 400.0,
        };
        let mut editor = AutomationEditor::default();
        editor.open();
        let entry_node = graph.node(entry).unwrap();
        let entry_rect = editor.node_rect(&zone, &graph, entry_node);
        assert_eq!(
            editor.handle_event(
                &press(line_output_rect(entry_rect, entry_node).unwrap()),
                &zone,
                &graph,
                &project,
            ),
            Some(EventResponse::Consumed)
        );
        assert_eq!(
            editor.handle_event(
                &UiEvent::MouseRelease { x: 500.0, y: 260.0 },
                &zone,
                &graph,
                &project,
            ),
            Some(EventResponse::Consumed)
        );
        let menu = editor.context_menu.as_ref().unwrap();
        assert!(
            matches!(menu.source, Some(PendingConnection::Line { node_id }) if node_id == entry)
        );
        let menu_rect = menu.rect;

        assert!(matches!(
            editor.handle_event(&press(menu_rect), &zone, &graph, &project),
            Some(EventResponse::Action(
                UiAction::AutomationAddConnectedNode {
                    kind: AutomationNodeKind::LineReroute,
                    from_node,
                    edge_kind: AutomationEdgeKind::Line,
                    ..
                }
            )) if from_node == entry
        ));
    }

    #[test]
    fn selected_node_is_deleted_by_delete_event() {
        let (graph, entry, _) = entry_and_condition();
        let project = Project::new();
        let zone = Rect {
            x: 0.0,
            y: 32.0,
            width: 800.0,
            height: 400.0,
        };
        let mut editor = AutomationEditor::default();
        editor.open();
        let rect = editor.node_rect(&zone, &graph, graph.node(entry).unwrap());
        editor.handle_event(&press(rect), &zone, &graph, &project);
        editor.handle_event(&release(rect), &zone, &graph, &project);
        assert_eq!(
            editor.handle_event(&UiEvent::Delete, &zone, &graph, &project),
            Some(EventResponse::Action(UiAction::AutomationDeleteNode {
                node_id: entry
            }))
        );
    }

    #[test]
    fn selected_node_is_deleted_by_windows_del_character() {
        let (graph, entry, _) = entry_and_condition();
        let project = Project::new();
        let zone = Rect {
            x: 0.0,
            y: 32.0,
            width: 800.0,
            height: 400.0,
        };
        let mut editor = AutomationEditor::default();
        editor.open();
        let rect = editor.node_rect(&zone, &graph, graph.node(entry).unwrap());
        editor.handle_event(&press(rect), &zone, &graph, &project);
        editor.handle_event(&release(rect), &zone, &graph, &project);

        assert_eq!(
            editor.handle_event(
                &UiEvent::KeyInput {
                    text: "\x7f".into()
                },
                &zone,
                &graph,
                &project
            ),
            Some(EventResponse::Action(UiAction::AutomationDeleteNode {
                node_id: entry
            }))
        );
    }

    #[test]
    fn automation_render_payload_is_clipped_to_the_video_zone() {
        let mut graph = AutomationGraph::default();
        let condition = graph
            .add_node(AutomationNodeKind::IfRole, 10.0, 10.0)
            .unwrap();
        for index in 0..24 {
            assert!(graph.add_role(condition, format!("Role {index}")));
        }
        let project = Project::new();
        let zone = Rect {
            x: 0.0,
            y: 32.0,
            width: 500.0,
            height: 120.0,
        };
        let mut editor = AutomationEditor::default();
        editor.open();
        let mut quads = Vec::new();
        let mut labels = Vec::new();
        editor.render(&zone, &graph, &project, (0.0, 0.0), &mut quads, &mut labels);

        for quad in quads
            .iter()
            .filter(|quad| quad.rotation.abs() <= f32::EPSILON)
        {
            assert!(quad.rect[0] >= zone.x);
            assert!(quad.rect[1] >= zone.y);
            assert!(quad.rect[0] + quad.rect[2] <= zone.x + zone.width + 0.01);
            assert!(quad.rect[1] + quad.rect[3] <= zone.y + zone.height + 0.01);
        }
        for info in labels {
            assert!(info.bounds.x >= zone.x);
            assert!(info.bounds.y >= zone.y);
            assert!(info.bounds.x + info.bounds.width <= zone.x + zone.width + 0.01);
            assert!(info.bounds.y + info.bounds.height <= zone.y + zone.height + 0.01);
        }
    }
}
