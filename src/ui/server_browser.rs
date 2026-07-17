#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_match)]

use super::primitives::{HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiEvent, VAlign};
use super::text_input;
use crate::i18n::t;

const CARD_W: f32 = 480.0;
const CARD_H: f32 = 420.0;
const SERVER_ITEM_H: f32 = 52.0;
const LIST_H: f32 = 260.0;
const BTN_H: f32 = 28.0;
const BTN_GAP: f32 = 6.0;
const FIELD_FONT_SIZE: f32 = 13.0;
const FIELD_PADDING_X: f32 = 8.0;

#[derive(Clone)]
pub struct ServerInfo {
    pub ip: String,
    pub port: u16,
    pub name: String,
    pub motd: String,
    pub online: u32,
    pub max_slots: u32,
    pub status: ServerStatus,
    pub players_text: String,
}

#[derive(Clone, PartialEq)]
pub enum ServerStatus {
    Pinging,
    Online,
    Offline,
}

pub enum BrowserResult {
    Consumed,
    Close,
    CreateRoom { ip: String, port: u16 },
    JoinRoom { ip: String, port: u16 },
    AddServer,
    RemoveServer(usize),
    Refresh,
}

pub struct ServerBrowserModal {
    pub servers: Vec<ServerInfo>,
    pub selected: Option<usize>,
    pub scroll_offset: f32,
    keyboard_focus: usize,
}

impl Default for ServerBrowserModal {
    fn default() -> Self {
        Self::new()
    }
}

impl ServerBrowserModal {
    pub fn new() -> Self {
        let saved = crate::config::saved_servers();
        let servers: Vec<ServerInfo> = saved
            .iter()
            .map(|s| ServerInfo {
                ip: s.ip.clone(),
                port: s.port,
                name: String::new(),
                motd: String::new(),
                online: 0,
                max_slots: 0,
                status: ServerStatus::Pinging,
                players_text: String::new(),
            })
            .collect();
        let selected = (!servers.is_empty()).then_some(0);
        Self {
            servers,
            selected,
            scroll_offset: 0.0,
            keyboard_focus: 0,
        }
    }

    pub fn keyboard_focus_label(&self) -> String {
        match self.keyboard_focus {
            0 => self
                .keyboard_selection_label()
                .unwrap_or_else(|| t("server_browser.empty").to_string()),
            1 => t("server_browser.create").to_string(),
            2 => t("server_browser.join").to_string(),
            3 => t("server_browser.refresh").to_string(),
            4 => t("server_browser.add").to_string(),
            5 => t("server_browser.remove").to_string(),
            _ => t("server_browser.close").to_string(),
        }
    }

    pub fn keyboard_selection_label(&self) -> Option<String> {
        let server = self.selected.and_then(|index| self.servers.get(index))?;
        let name = if server.name.trim().is_empty() {
            format!("{}: {}", server.ip, server.port)
        } else {
            server.name.clone()
        };
        let status = match server.status {
            ServerStatus::Online => t("server_browser.online"),
            ServerStatus::Offline => t("server_browser.offline"),
            ServerStatus::Pinging => t("server_browser.pinging"),
        };
        Some(format!("{name}, {status}, {}", server.players_text))
    }

    fn ensure_selected_visible(&mut self) {
        let Some(index) = self.selected else {
            return;
        };
        let top = index as f32 * SERVER_ITEM_H;
        let bottom = top + SERVER_ITEM_H;
        if top < self.scroll_offset {
            self.scroll_offset = top;
        } else if bottom > self.scroll_offset + LIST_H {
            self.scroll_offset = bottom - LIST_H;
        }
    }

    fn activate_keyboard_focus(&self) -> BrowserResult {
        match self.keyboard_focus {
            0 | 2 => self
                .selected
                .and_then(|index| self.servers.get(index))
                .filter(|server| server.status == ServerStatus::Online)
                .map(|server| BrowserResult::JoinRoom {
                    ip: server.ip.clone(),
                    port: server.port,
                })
                .unwrap_or(BrowserResult::Consumed),
            1 => self
                .selected
                .and_then(|index| self.servers.get(index))
                .filter(|server| server.status == ServerStatus::Online)
                .map(|server| BrowserResult::CreateRoom {
                    ip: server.ip.clone(),
                    port: server.port,
                })
                .unwrap_or(BrowserResult::Consumed),
            3 => BrowserResult::Refresh,
            4 => BrowserResult::AddServer,
            5 => self
                .selected
                .map(BrowserResult::RemoveServer)
                .unwrap_or(BrowserResult::Consumed),
            _ => BrowserResult::Close,
        }
    }

    pub fn update_server_info(
        &mut self,
        ip: &str,
        port: u16,
        name: String,
        motd: String,
        online: u32,
        max_slots: u32,
    ) {
        for s in &mut self.servers {
            if s.ip == ip && s.port == port {
                s.name = name.clone();
                s.motd = motd.clone();
                s.online = online;
                s.max_slots = max_slots;
                s.status = ServerStatus::Online;
                s.players_text = format!("{}/{}", online, max_slots);
            }
        }
    }

    pub fn mark_offline(&mut self, ip: &str, port: u16) {
        for s in &mut self.servers {
            if s.ip == ip && s.port == port {
                s.status = ServerStatus::Offline;
            }
        }
    }

    fn card_rect(sw: f32, sh: f32) -> Rect {
        Rect {
            x: (sw - CARD_W) / 2.0,
            y: (sh - CARD_H) / 2.0,
            width: CARD_W,
            height: CARD_H,
        }
    }

    fn list_rect(sw: f32, sh: f32) -> Rect {
        let card = Self::card_rect(sw, sh);
        Rect {
            x: card.x + 16.0,
            y: card.y + 40.0,
            width: card.width - 32.0,
            height: LIST_H,
        }
    }

    pub fn handle_event(&mut self, event: &UiEvent, sw: f32, sh: f32) -> BrowserResult {
        let card = Self::card_rect(sw, sh);
        let list = Self::list_rect(sw, sh);

        match event {
            UiEvent::KeyInput { text } if text == "\x1b" => BrowserResult::Close,
            UiEvent::KeyInput { text } if text == "\t" => {
                self.keyboard_focus = (self.keyboard_focus + 1) % 7;
                BrowserResult::Consumed
            }
            UiEvent::KeyInput { text } if text == "\u{b}" => {
                self.keyboard_focus = (self.keyboard_focus + 6) % 7;
                BrowserResult::Consumed
            }
            UiEvent::KeyInput { text } if text == "\r" || text == "\n" || text == " " => {
                self.activate_keyboard_focus()
            }
            UiEvent::CursorUp if self.keyboard_focus == 0 => {
                let current = self.selected.unwrap_or(0);
                self.selected = Some(current.saturating_sub(1));
                self.ensure_selected_visible();
                BrowserResult::Consumed
            }
            UiEvent::CursorDown if self.keyboard_focus == 0 => {
                if !self.servers.is_empty() {
                    let current = self.selected.unwrap_or(0);
                    self.selected = Some((current + 1).min(self.servers.len() - 1));
                    self.ensure_selected_visible();
                }
                BrowserResult::Consumed
            }
            UiEvent::Home if self.keyboard_focus == 0 => {
                self.selected = (!self.servers.is_empty()).then_some(0);
                self.ensure_selected_visible();
                BrowserResult::Consumed
            }
            UiEvent::End if self.keyboard_focus == 0 => {
                self.selected = (!self.servers.is_empty()).then_some(self.servers.len() - 1);
                self.ensure_selected_visible();
                BrowserResult::Consumed
            }

            UiEvent::Scroll { x, y, delta, .. } if list.contains(*x, *y) => {
                let max_scroll = (self.servers.len() as f32 * SERVER_ITEM_H - LIST_H).max(0.0);
                self.scroll_offset = (self.scroll_offset - delta * 30.0).clamp(0.0, max_scroll);
                BrowserResult::Consumed
            }

            UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
                if !card.contains(*x, *y) {
                    return BrowserResult::Close;
                }

                // Server list click
                if list.contains(*x, *y) {
                    self.keyboard_focus = 0;
                    let rel_y = *y - list.y + self.scroll_offset;
                    let idx = (rel_y / SERVER_ITEM_H) as usize;
                    if idx < self.servers.len() {
                        self.selected = Some(idx);
                    }
                    return BrowserResult::Consumed;
                }

                // Buttons (centered)
                let btns_y = card.y + 40.0 + LIST_H + 12.0;
                let bw = 80.0;
                let total_w = bw * 3.0 + BTN_GAP * 2.0;
                let bx = card.x + (card.width - total_w) / 2.0;

                // Row 1: Create | Join | Refresh
                if (Rect {
                    x: bx,
                    y: btns_y,
                    width: bw,
                    height: BTN_H,
                })
                .contains(*x, *y)
                {
                    self.keyboard_focus = 1;
                    if let Some(i) = self.selected {
                        let s = &self.servers[i];
                        if s.status == ServerStatus::Online {
                            return BrowserResult::CreateRoom {
                                ip: s.ip.clone(),
                                port: s.port,
                            };
                        }
                    }
                    return BrowserResult::Consumed;
                }
                if (Rect {
                    x: bx + bw + BTN_GAP,
                    y: btns_y,
                    width: bw,
                    height: BTN_H,
                })
                .contains(*x, *y)
                {
                    self.keyboard_focus = 2;
                    if let Some(i) = self.selected {
                        let s = &self.servers[i];
                        if s.status == ServerStatus::Online {
                            return BrowserResult::JoinRoom {
                                ip: s.ip.clone(),
                                port: s.port,
                            };
                        }
                    }
                    return BrowserResult::Consumed;
                }
                if (Rect {
                    x: bx + (bw + BTN_GAP) * 2.0,
                    y: btns_y,
                    width: bw,
                    height: BTN_H,
                })
                .contains(*x, *y)
                {
                    self.keyboard_focus = 3;
                    return BrowserResult::Refresh;
                }

                // Row 2: Add | Remove | Close
                let row2_y = btns_y + BTN_H + BTN_GAP;
                if (Rect {
                    x: bx,
                    y: row2_y,
                    width: bw,
                    height: BTN_H,
                })
                .contains(*x, *y)
                {
                    self.keyboard_focus = 4;
                    return BrowserResult::AddServer;
                }
                if (Rect {
                    x: bx + bw + BTN_GAP,
                    y: row2_y,
                    width: bw,
                    height: BTN_H,
                })
                .contains(*x, *y)
                {
                    self.keyboard_focus = 5;
                    if let Some(i) = self.selected {
                        return BrowserResult::RemoveServer(i);
                    }
                    return BrowserResult::Consumed;
                }
                if (Rect {
                    x: bx + (bw + BTN_GAP) * 2.0,
                    y: row2_y,
                    width: bw,
                    height: BTN_H,
                })
                .contains(*x, *y)
                {
                    self.keyboard_focus = 6;
                    return BrowserResult::Close;
                }

                BrowserResult::Consumed
            }
            _ => BrowserResult::Consumed,
        }
    }

    pub fn render<'a>(
        &'a self,
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        sw: f32,
        sh: f32,
    ) {
        let card = Self::card_rect(sw, sh);
        let list = Self::list_rect(sw, sh);

        // Dim
        quads.push(QuadInstance {
            rect: [0.0, 0.0, sw, sh],
            color: [0.0, 0.0, 0.0, 0.75],
            color_bottom: [0.0, 0.0, 0.0, 0.75],
            border_color: [0.0; 4],
            border_width: 0.0,
            border_radius: 0.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
        // Card
        quads.push(QuadInstance {
            rect: [card.x, card.y, card.width, card.height],
            color: [0.22, 0.22, 0.26, 1.0],
            color_bottom: [0.16, 0.16, 0.19, 1.0],
            border_color: [0.45, 0.45, 0.52, 0.8],
            border_width: 1.5,
            border_radius: 14.0,
            shadow_offset: [0.0, 4.0],
            shadow_color: [0.0, 0.0, 0.0, 0.5],
            shadow_blur: 10.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
        // Title
        labels.push(LabelInfo {
            text: t("server_browser.title"),
            bounds: Rect {
                x: card.x,
                y: card.y + 8.0,
                width: card.width,
                height: 28.0,
            },
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(16.0),
            color_override: None,
            font_family_override: None,
        });

        // List background
        quads.push(QuadInstance {
            rect: [list.x, list.y, list.width, list.height],
            color: [0.08, 0.08, 0.10, 1.0],
            color_bottom: [0.08, 0.08, 0.10, 1.0],
            border_color: if self.keyboard_focus == 0 {
                [0.45, 0.62, 0.95, 1.0]
            } else {
                [0.30, 0.30, 0.36, 0.5]
            },
            border_width: if self.keyboard_focus == 0 { 2.0 } else { 1.0 },
            border_radius: 4.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });

        // Server items
        let first = (self.scroll_offset / SERVER_ITEM_H) as usize;
        let visible = (LIST_H / SERVER_ITEM_H) as usize + 2;
        for i in first..self.servers.len().min(first + visible) {
            let iy = list.y + (i as f32 * SERVER_ITEM_H) - self.scroll_offset;
            if iy + SERVER_ITEM_H < list.y || iy > list.y + LIST_H {
                continue;
            }

            let is_selected = self.selected == Some(i);
            if is_selected {
                quads.push(QuadInstance {
                    rect: [
                        list.x + 2.0,
                        iy.max(list.y),
                        list.width - 4.0,
                        SERVER_ITEM_H.min(list.y + LIST_H - iy),
                    ],
                    color: [0.30, 0.28, 0.55, 0.6],
                    color_bottom: [0.30, 0.28, 0.55, 0.6],
                    border_color: [0.0; 4],
                    border_width: 0.0,
                    border_radius: 3.0,
                    shadow_offset: [0.0; 2],
                    shadow_color: [0.0; 4],
                    shadow_blur: 0.0,
                    rotation: 0.0,
                    _padding: [0.0; 2],
                });
            }

            let s = &self.servers[i];
            // Status dot
            let dot_color = match s.status {
                ServerStatus::Online => [0.2, 0.8, 0.3, 1.0],
                ServerStatus::Offline => [0.8, 0.2, 0.2, 1.0],
                ServerStatus::Pinging => [0.7, 0.7, 0.3, 1.0],
            };
            quads.push(QuadInstance {
                rect: [list.x + 10.0, iy + 8.0, 8.0, 8.0],
                color: dot_color,
                color_bottom: dot_color,
                border_color: [0.0; 4],
                border_width: 0.0,
                border_radius: 4.0,
                shadow_offset: [0.0; 2],
                shadow_color: [0.0; 4],
                shadow_blur: 0.0,
                rotation: 0.0,
                _padding: [0.0; 2],
            });

            if iy >= list.y - SERVER_ITEM_H && iy < list.y + LIST_H {
                // Line 1: server name (never show IP)
                let display_name = if s.name.is_empty() { "..." } else { &s.name };
                labels.push(LabelInfo {
                    text: display_name,
                    bounds: Rect {
                        x: list.x + 24.0,
                        y: iy + 2.0,
                        width: list.width - 100.0,
                        height: 20.0,
                    },
                    h_align: HAlign::Left,
                    v_align: VAlign::Center,
                    overflow: Overflow::Ellipsis,
                    padding: 0.0,
                    font_size_override: Some(12.0),
                    color_override: None,
                    font_family_override: None,
                });
                // Line 1 right: players count
                labels.push(LabelInfo {
                    text: &s.players_text,
                    bounds: Rect {
                        x: list.x + list.width - 80.0,
                        y: iy + 2.0,
                        width: 70.0,
                        height: 20.0,
                    },
                    h_align: HAlign::Right,
                    v_align: VAlign::Center,
                    overflow: Overflow::Clip,
                    padding: 0.0,
                    font_size_override: Some(10.0),
                    color_override: Some([160, 160, 175]),
                    font_family_override: None,
                });
                // Line 2: motd or status
                let info_text = if s.status == ServerStatus::Online {
                    &s.motd
                } else if s.status == ServerStatus::Pinging {
                    "..."
                } else {
                    "Hors ligne"
                };
                labels.push(LabelInfo {
                    text: info_text,
                    bounds: Rect {
                        x: list.x + 24.0,
                        y: iy + 22.0,
                        width: list.width - 30.0,
                        height: 16.0,
                    },
                    h_align: HAlign::Left,
                    v_align: VAlign::Center,
                    overflow: Overflow::Ellipsis,
                    padding: 0.0,
                    font_size_override: Some(10.0),
                    color_override: Some([130, 130, 145]),
                    font_family_override: None,
                });
            }
        }

        // Buttons (centered)
        let btns_y = card.y + 40.0 + LIST_H + 12.0;
        let bw = 80.0;
        let total_w = bw * 3.0 + BTN_GAP * 2.0;
        let bx = card.x + (card.width - total_w) / 2.0;

        let buttons_row1 = [
            (t("server_browser.create"), self.selected.is_none()),
            (t("server_browser.join"), self.selected.is_none()),
            (t("server_browser.refresh"), false),
        ];
        let buttons_row2 = [
            (t("server_browser.add"), false),
            (t("server_browser.remove"), self.selected.is_none()),
            (t("server_browser.close"), false),
        ];

        for (row_idx, buttons) in [&buttons_row1, &buttons_row2].iter().enumerate() {
            let row_y = btns_y + row_idx as f32 * (BTN_H + BTN_GAP);
            for (i, (label, disabled)) in buttons.iter().enumerate() {
                let bxi = bx + i as f32 * (bw + BTN_GAP);
                let bg = if *disabled {
                    [0.12, 0.12, 0.14, 1.0]
                } else {
                    [0.18, 0.18, 0.22, 1.0]
                };
                let focused = self.keyboard_focus == row_idx * 3 + i + 1;
                let border = if focused {
                    [0.45, 0.62, 0.95, 1.0]
                } else if *disabled {
                    [0.20, 0.20, 0.24, 0.5]
                } else {
                    [0.35, 0.35, 0.42, 0.6]
                };
                quads.push(QuadInstance {
                    rect: [bxi, row_y, bw, BTN_H],
                    color: bg,
                    color_bottom: bg,
                    border_color: border,
                    border_width: if focused { 2.0 } else { 1.0 },
                    border_radius: 4.0,
                    shadow_offset: [0.0; 2],
                    shadow_color: [0.0; 4],
                    shadow_blur: 0.0,
                    rotation: 0.0,
                    _padding: [0.0; 2],
                });
                let text_color = if *disabled { Some([90, 90, 100]) } else { None };
                labels.push(LabelInfo {
                    text: label,
                    bounds: Rect {
                        x: bxi,
                        y: row_y,
                        width: bw,
                        height: BTN_H,
                    },
                    h_align: HAlign::Center,
                    v_align: VAlign::Center,
                    overflow: Overflow::Clip,
                    padding: 0.0,
                    font_size_override: Some(11.0),
                    color_override: text_color,
                    font_family_override: None,
                });
            }
        }
    }
}

// --- Add Server Modal ---

pub struct AddServerModal {
    pub ip: String,
    pub port: String,
    pub input: text_input::TextInputState,
    pub focused: usize, // 0=ip, 1=port, 2=add button
}

pub enum AddServerResult {
    Consumed,
    Close,
    Add { ip: String, port: u16 },
}

impl Default for AddServerModal {
    fn default() -> Self {
        Self::new()
    }
}

impl AddServerModal {
    pub fn new() -> Self {
        let mut input = text_input::TextInputState::new();
        let ip = String::new();
        input.activate(&ip);
        Self {
            ip,
            port: "9050".into(),
            input,
            focused: 0,
        }
    }

    pub fn keyboard_focus_label(&self) -> String {
        match self.focused {
            0 => t("server_browser.ip").to_string(),
            1 => format!("{} : {}", t("server_browser.port"), self.port),
            _ => t("server_browser.add").to_string(),
        }
    }

    pub fn keyboard_focus_role(&self) -> &'static str {
        if self.focused == 2 {
            "button"
        } else {
            "text field"
        }
    }

    fn set_keyboard_focus(&mut self, focus: usize) {
        self.focused = focus % 3;
        if self.focused == 0 {
            self.input.activate(&self.ip);
        } else if self.focused == 1 {
            self.input.activate(&self.port);
        } else {
            self.input.deactivate();
        }
    }

    fn card_rect(sw: f32, sh: f32) -> Rect {
        let w = 340.0;
        let h = 180.0;
        Rect {
            x: (sw - w) / 2.0,
            y: (sh - h) / 2.0,
            width: w,
            height: h,
        }
    }

    pub fn handle_event(&mut self, event: &UiEvent, sw: f32, sh: f32) -> AddServerResult {
        let card = Self::card_rect(sw, sh);
        match event {
            UiEvent::KeyInput { text } => {
                if text == "\x1b" {
                    return AddServerResult::Close;
                }
                if text == "\t" {
                    self.set_keyboard_focus(self.focused + 1);
                    return AddServerResult::Consumed;
                }
                if text == "\u{b}" {
                    self.set_keyboard_focus(self.focused + 2);
                    return AddServerResult::Consumed;
                }
                if text == "\r" || text == "\n" {
                    let ip = self.ip.trim().to_string();
                    let port: u16 = self.port.trim().parse().unwrap_or(9050);
                    if !ip.is_empty() {
                        return AddServerResult::Add { ip, port };
                    }
                    return AddServerResult::Consumed;
                }
                if self.focused >= 2 {
                    return AddServerResult::Consumed;
                }
                let field = if self.focused == 0 {
                    &self.ip
                } else {
                    &self.port
                };
                if let Some(action) = self.input.handle_key(text, field) {
                    if let text_input::TextInputAction::Changed(new_text) = action {
                        if self.focused == 0 {
                            self.ip = new_text;
                        } else {
                            self.port = new_text;
                        }
                    }
                }
                AddServerResult::Consumed
            }
            UiEvent::CursorLeft => {
                self.input.move_left();
                AddServerResult::Consumed
            }
            UiEvent::CursorRight => {
                let f = if self.focused == 0 {
                    &self.ip
                } else {
                    &self.port
                };
                self.input.move_right(f);
                AddServerResult::Consumed
            }
            UiEvent::CursorUp | UiEvent::CursorDown => {
                let step = if matches!(event, UiEvent::CursorUp) {
                    2
                } else {
                    1
                };
                self.set_keyboard_focus(self.focused + step);
                AddServerResult::Consumed
            }
            UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
                let double = matches!(event, UiEvent::DoubleClick { .. });
                if !card.contains(*x, *y) {
                    return AddServerResult::Close;
                }
                // Check field clicks
                let fx = card.x + 20.0;
                let fw = card.width - 40.0;
                let fh = 28.0;
                let ip_rect = Rect {
                    x: fx,
                    y: card.y + 56.0,
                    width: fw,
                    height: fh,
                };
                let port_rect = Rect {
                    x: fx,
                    y: card.y + 104.0,
                    width: fw,
                    height: fh,
                };
                if ip_rect.contains(*x, *y) {
                    self.focused = 0;
                    self.input.activate(&self.ip);
                    if double {
                        self.input.select_all(&self.ip);
                    } else {
                        let pos =
                            text_input::cursor_pos_from_x(&self.ip, ip_rect, *x, field_metrics());
                        self.input.set_cursor_pos(pos);
                    }
                }
                if port_rect.contains(*x, *y) {
                    self.focused = 1;
                    self.input.activate(&self.port);
                    if double {
                        self.input.select_all(&self.port);
                    } else {
                        let pos = text_input::cursor_pos_from_x(
                            &self.port,
                            port_rect,
                            *x,
                            field_metrics(),
                        );
                        self.input.set_cursor_pos(pos);
                    }
                }
                // Add button
                let btn = Rect {
                    x: card.x + (card.width - 120.0) / 2.0,
                    y: card.y + 142.0,
                    width: 120.0,
                    height: 28.0,
                };
                if btn.contains(*x, *y) {
                    self.set_keyboard_focus(2);
                    let ip = self.ip.trim().to_string();
                    let port: u16 = self.port.trim().parse().unwrap_or(9050);
                    if !ip.is_empty() {
                        return AddServerResult::Add { ip, port };
                    }
                }
                AddServerResult::Consumed
            }
            _ => AddServerResult::Consumed,
        }
    }

    pub fn render<'a>(
        &'a self,
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        sw: f32,
        sh: f32,
    ) {
        let card = Self::card_rect(sw, sh);
        // Dim
        quads.push(QuadInstance {
            rect: [0.0, 0.0, sw, sh],
            color: [0.0, 0.0, 0.0, 0.75],
            color_bottom: [0.0, 0.0, 0.0, 0.75],
            border_color: [0.0; 4],
            border_width: 0.0,
            border_radius: 0.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
        quads.push(QuadInstance {
            rect: [card.x, card.y, card.width, card.height],
            color: [0.22, 0.22, 0.26, 1.0],
            color_bottom: [0.16, 0.16, 0.19, 1.0],
            border_color: [0.45, 0.45, 0.52, 0.8],
            border_width: 1.5,
            border_radius: 14.0,
            shadow_offset: [0.0, 4.0],
            shadow_color: [0.0, 0.0, 0.0, 0.5],
            shadow_blur: 10.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
        labels.push(LabelInfo {
            text: t("server_browser.add_title"),
            bounds: Rect {
                x: card.x,
                y: card.y + 8.0,
                width: card.width,
                height: 24.0,
            },
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(15.0),
            color_override: None,
            font_family_override: None,
        });

        let fx = card.x + 20.0;
        let fw = card.width - 40.0;
        let fh = 28.0;
        let fields = [
            ("IP", card.y + 40.0, &self.ip),
            ("Port", card.y + 88.0, &self.port),
        ];
        for (i, (label, ly, value)) in fields.iter().enumerate() {
            labels.push(LabelInfo {
                text: label,
                bounds: Rect {
                    x: fx,
                    y: *ly,
                    width: fw,
                    height: 14.0,
                },
                h_align: HAlign::Left,
                v_align: VAlign::Center,
                overflow: Overflow::Clip,
                padding: 0.0,
                font_size_override: Some(11.0),
                color_override: Some(if self.focused == i {
                    [200, 200, 220]
                } else {
                    [140, 140, 155]
                }),
                font_family_override: None,
            });
            let iy = *ly + 16.0;
            let border = if self.focused == i {
                [0.40, 0.37, 0.80, 0.8]
            } else {
                [0.30, 0.30, 0.36, 0.5]
            };
            quads.push(QuadInstance {
                rect: [fx, iy, fw, fh],
                color: [0.08, 0.08, 0.10, 1.0],
                color_bottom: [0.08, 0.08, 0.10, 1.0],
                border_color: border,
                border_width: 1.0,
                border_radius: 4.0,
                shadow_offset: [0.0; 2],
                shadow_color: [0.0; 4],
                shadow_blur: 0.0,
                rotation: 0.0,
                _padding: [0.0; 2],
            });
            if !value.is_empty() {
                labels.push(LabelInfo {
                    text: value,
                    bounds: Rect {
                        x: fx,
                        y: iy,
                        width: fw,
                        height: fh,
                    },
                    h_align: HAlign::Left,
                    v_align: VAlign::Center,
                    overflow: Overflow::Clip,
                    padding: FIELD_PADDING_X,
                    font_size_override: Some(FIELD_FONT_SIZE),
                    color_override: None,
                    font_family_override: None,
                });
            }
            text_input::render_selection_and_cursor(
                quads,
                Rect {
                    x: fx,
                    y: iy,
                    width: fw,
                    height: fh,
                },
                value,
                &self.input,
                self.focused == i,
                field_metrics(),
                4.0,
                4.0,
                [0.25, 0.45, 0.95, 0.42],
                [0.90, 0.90, 0.96, 1.0],
            );
        }
        // Add button
        let btn = Rect {
            x: card.x + (card.width - 120.0) / 2.0,
            y: card.y + 142.0,
            width: 120.0,
            height: 28.0,
        };
        quads.push(QuadInstance {
            rect: [btn.x, btn.y, btn.width, btn.height],
            color: [0.30, 0.55, 0.30, 1.0],
            color_bottom: [0.22, 0.45, 0.22, 1.0],
            border_color: [0.40, 0.65, 0.40, 0.8],
            border_width: 1.0,
            border_radius: 6.0,
            shadow_offset: [0.0, 2.0],
            shadow_color: [0.0, 0.0, 0.0, 0.3],
            shadow_blur: 4.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
        labels.push(LabelInfo {
            text: t("server_browser.add"),
            bounds: Rect {
                x: btn.x,
                y: btn.y,
                width: btn.width,
                height: btn.height,
            },
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(12.0),
            color_override: None,
            font_family_override: None,
        });
    }
}

fn field_metrics() -> text_input::TextInputMetrics {
    text_input::TextInputMetrics::left(FIELD_FONT_SIZE, FIELD_PADDING_X)
}
