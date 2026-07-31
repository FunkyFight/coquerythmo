#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_match)]

use super::primitives::{HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiEvent, VAlign};
use super::text_input;
use crate::i18n::t;

const CARD_W: f32 = 720.0;
const CARD_H: f32 = 584.0;
const CARD_PADDING: f32 = 28.0;
const SERVER_ITEM_H: f32 = 68.0;
const LIST_H: f32 = 340.0;
const MANAGE_BTN_H: f32 = 38.0;
const PRIMARY_BTN_H: f32 = 46.0;
const FIELD_FONT_SIZE: f32 = 15.0;
const FIELD_PADDING_X: f32 = 12.0;
const ADD_CARD_W: f32 = 560.0;
const ADD_CARD_H: f32 = 360.0;
const ADD_FIELD_H: f32 = 44.0;

#[derive(Clone)]
pub struct ServerInfo {
    pub ip: String,
    pub port: u16,
    pub endpoint: String,
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
                endpoint: format!("{}:{}", s.ip, s.port),
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
            5 if self.selected_default() => t("server_browser.default").to_string(),
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
                .filter(|_| self.can_remove_selected())
                .unwrap_or(BrowserResult::Consumed),
            _ => BrowserResult::Close,
        }
    }

    fn selected_online(&self) -> bool {
        self.selected
            .and_then(|index| self.servers.get(index))
            .is_some_and(|server| server.status == ServerStatus::Online)
    }

    fn selected_default(&self) -> bool {
        self.selected
            .and_then(|index| self.servers.get(index))
            .is_some_and(|server| crate::config::is_default_server(&server.ip, server.port))
    }

    fn can_remove_selected(&self) -> bool {
        self.selected.is_some() && !self.selected_default()
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
            x: card.x + CARD_PADDING,
            y: card.y + 100.0,
            width: card.width - CARD_PADDING * 2.0,
            height: LIST_H,
        }
    }

    fn manage_button_rects(card: Rect) -> [Rect; 4] {
        let y = card.y + 458.0;
        [
            Rect {
                x: card.x + CARD_PADDING,
                y,
                width: 156.0,
                height: MANAGE_BTN_H,
            },
            Rect {
                x: card.x + CARD_PADDING + 168.0,
                y,
                width: 112.0,
                height: MANAGE_BTN_H,
            },
            Rect {
                x: card.x + CARD_PADDING + 292.0,
                y,
                width: 124.0,
                height: MANAGE_BTN_H,
            },
            Rect {
                x: card.x + card.width - CARD_PADDING - 100.0,
                y,
                width: 100.0,
                height: MANAGE_BTN_H,
            },
        ]
    }

    fn primary_button_rects(card: Rect) -> [Rect; 2] {
        let width = 218.0;
        let gap = 12.0;
        let x = card.x + (card.width - width * 2.0 - gap) / 2.0;
        [
            Rect {
                x,
                y: card.y + 516.0,
                width,
                height: PRIMARY_BTN_H,
            },
            Rect {
                x: x + width + gap,
                y: card.y + 516.0,
                width,
                height: PRIMARY_BTN_H,
            },
        ]
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
            UiEvent::FocusNext => {
                self.keyboard_focus = (self.keyboard_focus + 1) % 7;
                BrowserResult::Consumed
            }
            UiEvent::FocusPrevious => {
                self.keyboard_focus = (self.keyboard_focus + 6) % 7;
                BrowserResult::Consumed
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
                let double = matches!(event, UiEvent::DoubleClick { .. });
                if !card.contains(*x, *y) {
                    return BrowserResult::Close;
                }

                if list.contains(*x, *y) {
                    self.keyboard_focus = 0;
                    let rel_y = *y - list.y + self.scroll_offset;
                    let idx = (rel_y / SERVER_ITEM_H) as usize;
                    if idx < self.servers.len() {
                        self.selected = Some(idx);
                        if double && self.servers[idx].status == ServerStatus::Online {
                            return BrowserResult::JoinRoom {
                                ip: self.servers[idx].ip.clone(),
                                port: self.servers[idx].port,
                            };
                        }
                    }
                    return BrowserResult::Consumed;
                }

                let [add, remove, refresh, close] = Self::manage_button_rects(card);
                for (rect, focus) in [(add, 4), (remove, 5), (refresh, 3), (close, 6)] {
                    if rect.contains(*x, *y) {
                        self.keyboard_focus = focus;
                        return match focus {
                            3 => BrowserResult::Refresh,
                            4 => BrowserResult::AddServer,
                            5 => self
                                .selected
                                .map(BrowserResult::RemoveServer)
                                .filter(|_| self.can_remove_selected())
                                .unwrap_or(BrowserResult::Consumed),
                            _ => BrowserResult::Close,
                        };
                    }
                }

                let [create, join] = Self::primary_button_rects(card);
                if create.contains(*x, *y) {
                    self.keyboard_focus = 1;
                    return self.activate_keyboard_focus();
                }
                if join.contains(*x, *y) {
                    self.keyboard_focus = 2;
                    return self.activate_keyboard_focus();
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
        quads.push(quad(
            Rect {
                x: 0.0,
                y: 0.0,
                width: sw,
                height: sh,
            },
            [0.0, 0.0, 0.0, 0.78],
            [0.0; 4],
            0.0,
            0.0,
        ));
        quads.push(card_quad(card));
        labels.push(label(
            t("server_browser.title"),
            Rect {
                x: card.x + CARD_PADDING,
                y: card.y + 16.0,
                width: card.width - CARD_PADDING * 2.0,
                height: 32.0,
            },
            HAlign::Left,
            22.0,
            None,
        ));
        labels.push(label(
            t("server_browser.description"),
            Rect {
                x: card.x + CARD_PADDING,
                y: card.y + 50.0,
                width: card.width - CARD_PADDING * 2.0,
                height: 22.0,
            },
            HAlign::Left,
            12.0,
            Some([174, 174, 190]),
        ));
        labels.push(label(
            t("server_browser.saved"),
            Rect {
                x: list.x,
                y: list.y - 24.0,
                width: list.width,
                height: 20.0,
            },
            HAlign::Left,
            12.0,
            Some([194, 194, 208]),
        ));
        quads.push(quad(
            list,
            [0.075, 0.075, 0.10, 1.0],
            if self.keyboard_focus == 0 {
                [0.45, 0.62, 0.95, 1.0]
            } else {
                [0.30, 0.30, 0.38, 0.9]
            },
            if self.keyboard_focus == 0 { 2.0 } else { 1.0 },
            9.0,
        ));

        if self.servers.is_empty() {
            labels.push(label(
                t("server_browser.empty"),
                Rect {
                    x: list.x + 32.0,
                    y: list.y + 116.0,
                    width: list.width - 64.0,
                    height: 28.0,
                },
                HAlign::Center,
                17.0,
                Some([214, 214, 225]),
            ));
            labels.push(label(
                t("server_browser.empty_hint"),
                Rect {
                    x: list.x + 32.0,
                    y: list.y + 148.0,
                    width: list.width - 64.0,
                    height: 24.0,
                },
                HAlign::Center,
                12.0,
                Some([135, 135, 153]),
            ));
        }

        let first = (self.scroll_offset / SERVER_ITEM_H) as usize;
        let visible = (LIST_H / SERVER_ITEM_H) as usize + 2;
        for i in first..self.servers.len().min(first + visible) {
            let iy = list.y + (i as f32 * SERVER_ITEM_H) - self.scroll_offset;
            if iy + SERVER_ITEM_H < list.y || iy > list.y + LIST_H {
                continue;
            }

            let is_selected = self.selected == Some(i);
            if is_selected {
                let top = iy.max(list.y + 2.0);
                let bottom = (iy + SERVER_ITEM_H).min(list.y + list.height - 2.0);
                quads.push(quad(
                    Rect {
                        x: list.x + 3.0,
                        y: top,
                        width: list.width - 6.0,
                        height: (bottom - top).max(0.0),
                    },
                    [0.29, 0.30, 0.58, 0.78],
                    [0.47, 0.52, 0.88, 0.75],
                    1.0,
                    6.0,
                ));
            } else if iy + SERVER_ITEM_H <= list.y + list.height {
                quads.push(quad(
                    Rect {
                        x: list.x + 16.0,
                        y: iy + SERVER_ITEM_H - 1.0,
                        width: list.width - 32.0,
                        height: 1.0,
                    },
                    [0.24, 0.24, 0.30, 0.55],
                    [0.0; 4],
                    0.0,
                    0.0,
                ));
            }

            let s = &self.servers[i];
            let dot_color = match s.status {
                ServerStatus::Online => [0.22, 0.84, 0.42, 1.0],
                ServerStatus::Offline => [0.88, 0.34, 0.34, 1.0],
                ServerStatus::Pinging => [0.92, 0.70, 0.24, 1.0],
            };
            quads.push(quad(
                Rect {
                    x: list.x + 16.0,
                    y: iy + 17.0,
                    width: 10.0,
                    height: 10.0,
                },
                dot_color,
                [0.0; 4],
                0.0,
                5.0,
            ));

            if iy >= list.y - SERVER_ITEM_H && iy < list.y + LIST_H {
                let display_name = if s.name.trim().is_empty() {
                    &s.endpoint
                } else {
                    &s.name
                };
                labels.push(label(
                    display_name,
                    Rect {
                        x: list.x + 36.0,
                        y: iy + 7.0,
                        width: list.width - 270.0,
                        height: 24.0,
                    },
                    HAlign::Left,
                    15.0,
                    Some([232, 232, 240]),
                ));
                let status = match s.status {
                    ServerStatus::Online => t("server_browser.online"),
                    ServerStatus::Offline => t("server_browser.offline"),
                    ServerStatus::Pinging => t("server_browser.pinging"),
                };
                labels.push(label(
                    status,
                    Rect {
                        x: list.x + list.width - 220.0,
                        y: iy + 9.0,
                        width: 130.0,
                        height: 20.0,
                    },
                    HAlign::Right,
                    12.0,
                    Some(match s.status {
                        ServerStatus::Online => [115, 220, 145],
                        ServerStatus::Offline => [235, 130, 130],
                        ServerStatus::Pinging => [225, 190, 105],
                    }),
                ));
                if !s.players_text.is_empty() {
                    labels.push(label(
                        &s.players_text,
                        Rect {
                            x: list.x + list.width - 80.0,
                            y: iy + 9.0,
                            width: 62.0,
                            height: 20.0,
                        },
                        HAlign::Right,
                        12.0,
                        Some([185, 185, 201]),
                    ));
                }
                let info_text = if s.status == ServerStatus::Online {
                    if s.motd.trim().is_empty() {
                        t("server_browser.ready")
                    } else {
                        &s.motd
                    }
                } else {
                    status
                };
                labels.push(label(
                    info_text,
                    Rect {
                        x: list.x + 36.0,
                        y: iy + 35.0,
                        width: list.width - 280.0,
                        height: 20.0,
                    },
                    HAlign::Left,
                    11.0,
                    Some([145, 145, 163]),
                ));
                labels.push(label(
                    &s.endpoint,
                    Rect {
                        x: list.x + list.width - 230.0,
                        y: iy + 35.0,
                        width: 212.0,
                        height: 20.0,
                    },
                    HAlign::Right,
                    11.0,
                    Some([132, 132, 151]),
                ));
            }
        }

        let [add, remove, refresh, close] = Self::manage_button_rects(card);
        render_button(
            quads,
            labels,
            add,
            t("server_browser.add"),
            false,
            true,
            self.keyboard_focus == 4,
        );
        render_button(
            quads,
            labels,
            remove,
            if self.selected_default() {
                t("server_browser.default")
            } else {
                t("server_browser.remove")
            },
            false,
            self.can_remove_selected(),
            self.keyboard_focus == 5,
        );
        render_button(
            quads,
            labels,
            refresh,
            t("server_browser.refresh"),
            false,
            true,
            self.keyboard_focus == 3,
        );
        render_button(
            quads,
            labels,
            close,
            t("server_browser.close"),
            false,
            true,
            self.keyboard_focus == 6,
        );

        let [create, join] = Self::primary_button_rects(card);
        render_button(
            quads,
            labels,
            create,
            t("server_browser.create"),
            false,
            self.selected_online(),
            self.keyboard_focus == 1,
        );
        render_button(
            quads,
            labels,
            join,
            t("server_browser.join"),
            true,
            self.selected_online(),
            self.keyboard_focus == 2,
        );
    }
}

// --- Add Server Modal ---

pub struct AddServerModal {
    pub ip: String,
    pub port: String,
    pub input: text_input::TextInputState,
    pub focused: usize, // 0=ip, 1=port, 2=cancel, 3=add
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
            2 => t("server_browser.cancel").to_string(),
            _ => t("server_browser.add_confirm").to_string(),
        }
    }

    pub fn keyboard_focus_role(&self) -> &'static str {
        if self.focused < 2 {
            "text field"
        } else {
            "button"
        }
    }

    fn set_keyboard_focus(&mut self, focus: usize) {
        self.focused = focus % 4;
        if self.focused == 0 {
            self.input.activate(&self.ip);
        } else if self.focused == 1 {
            self.input.activate(&self.port);
        } else {
            self.input.deactivate();
        }
    }

    fn card_rect(sw: f32, sh: f32) -> Rect {
        Rect {
            x: (sw - ADD_CARD_W) / 2.0,
            y: (sh - ADD_CARD_H) / 2.0,
            width: ADD_CARD_W,
            height: ADD_CARD_H,
        }
    }

    fn field_rects(card: Rect) -> [Rect; 2] {
        [
            Rect {
                x: card.x + CARD_PADDING,
                y: card.y + 124.0,
                width: card.width - CARD_PADDING * 2.0,
                height: ADD_FIELD_H,
            },
            Rect {
                x: card.x + CARD_PADDING,
                y: card.y + 210.0,
                width: card.width - CARD_PADDING * 2.0,
                height: ADD_FIELD_H,
            },
        ]
    }

    fn button_rects(card: Rect) -> [Rect; 2] {
        [
            Rect {
                x: card.x + card.width - CARD_PADDING - 302.0,
                y: card.y + 292.0,
                width: 120.0,
                height: 44.0,
            },
            Rect {
                x: card.x + card.width - CARD_PADDING - 170.0,
                y: card.y + 292.0,
                width: 170.0,
                height: 44.0,
            },
        ]
    }

    fn can_add(&self) -> bool {
        !self.ip.trim().is_empty() && self.port.trim().parse::<u16>().is_ok_and(|port| port > 0)
    }

    fn add_result(&self) -> AddServerResult {
        let Ok(port) = self.port.trim().parse::<u16>() else {
            return AddServerResult::Consumed;
        };
        let ip = self.ip.trim().to_string();
        if ip.is_empty() || port == 0 {
            AddServerResult::Consumed
        } else {
            AddServerResult::Add { ip, port }
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
                    self.set_keyboard_focus(self.focused + 3);
                    return AddServerResult::Consumed;
                }
                if text == "\r" || text == "\n" {
                    return if self.focused == 2 {
                        AddServerResult::Close
                    } else {
                        self.add_result()
                    };
                }
                if text == " " && self.focused >= 2 {
                    return if self.focused == 2 {
                        AddServerResult::Close
                    } else {
                        self.add_result()
                    };
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
            UiEvent::FocusNext => {
                self.set_keyboard_focus(self.focused + 1);
                AddServerResult::Consumed
            }
            UiEvent::FocusPrevious => {
                self.set_keyboard_focus(self.focused + 3);
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
                    3
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
                let [ip_rect, port_rect] = Self::field_rects(card);
                if ip_rect.contains(*x, *y) {
                    self.set_keyboard_focus(0);
                    if double {
                        self.input.select_all(&self.ip);
                    } else {
                        let pos =
                            text_input::cursor_pos_from_x(&self.ip, ip_rect, *x, field_metrics());
                        self.input.set_cursor_pos(pos);
                    }
                }
                if port_rect.contains(*x, *y) {
                    self.set_keyboard_focus(1);
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
                let [cancel, add] = Self::button_rects(card);
                if cancel.contains(*x, *y) {
                    self.set_keyboard_focus(2);
                    return AddServerResult::Close;
                }
                if add.contains(*x, *y) {
                    self.set_keyboard_focus(3);
                    return self.add_result();
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
        quads.push(quad(
            Rect {
                x: 0.0,
                y: 0.0,
                width: sw,
                height: sh,
            },
            [0.0, 0.0, 0.0, 0.78],
            [0.0; 4],
            0.0,
            0.0,
        ));
        quads.push(card_quad(card));
        labels.push(label(
            t("server_browser.add_title"),
            Rect {
                x: card.x + CARD_PADDING,
                y: card.y + 18.0,
                width: card.width - CARD_PADDING * 2.0,
                height: 32.0,
            },
            HAlign::Left,
            22.0,
            None,
        ));
        labels.push(label(
            t("server_browser.add_description"),
            Rect {
                x: card.x + CARD_PADDING,
                y: card.y + 52.0,
                width: card.width - CARD_PADDING * 2.0,
                height: 24.0,
            },
            HAlign::Left,
            12.0,
            Some([174, 174, 190]),
        ));

        let [ip_rect, port_rect] = Self::field_rects(card);
        for (index, (field, title, value, placeholder)) in [
            (
                ip_rect,
                t("server_browser.ip"),
                self.ip.as_str(),
                t("server_browser.ip_placeholder"),
            ),
            (
                port_rect,
                t("server_browser.port"),
                self.port.as_str(),
                t("server_browser.port_placeholder"),
            ),
        ]
        .into_iter()
        .enumerate()
        {
            labels.push(label(
                title,
                Rect {
                    x: field.x,
                    y: field.y - 24.0,
                    width: field.width,
                    height: 20.0,
                },
                HAlign::Left,
                13.0,
                Some(if self.focused == index {
                    [210, 218, 244]
                } else {
                    [180, 180, 198]
                }),
            ));
            quads.push(input_quad(field, self.focused == index));
            labels.push(LabelInfo {
                text: if value.is_empty() { placeholder } else { value },
                bounds: field,
                h_align: HAlign::Left,
                v_align: VAlign::Center,
                overflow: Overflow::Ellipsis,
                padding: FIELD_PADDING_X,
                font_size_override: Some(FIELD_FONT_SIZE),
                color_override: Some(if value.is_empty() {
                    [105, 105, 122]
                } else {
                    [232, 232, 240]
                }),
                font_family_override: None,
            });
            text_input::render_selection_and_cursor(
                quads,
                field,
                value,
                &self.input,
                self.focused == index,
                field_metrics(),
                6.0,
                6.0,
                [0.25, 0.45, 0.95, 0.42],
                [0.90, 0.90, 0.96, 1.0],
            );
        }

        labels.push(label(
            if !self.port.trim().is_empty() && self.port.trim().parse::<u16>().is_err() {
                t("server_browser.port_invalid")
            } else {
                t("server_browser.add_hint")
            },
            Rect {
                x: card.x + CARD_PADDING,
                y: card.y + 258.0,
                width: card.width - CARD_PADDING * 2.0,
                height: 20.0,
            },
            HAlign::Left,
            11.0,
            Some(
                if !self.port.trim().is_empty() && self.port.trim().parse::<u16>().is_err() {
                    [235, 130, 130]
                } else {
                    [140, 140, 158]
                },
            ),
        ));

        let [cancel, add] = Self::button_rects(card);
        render_button(
            quads,
            labels,
            cancel,
            t("server_browser.cancel"),
            false,
            true,
            self.focused == 2,
        );
        render_button(
            quads,
            labels,
            add,
            t("server_browser.add_confirm"),
            true,
            self.can_add(),
            self.focused == 3,
        );
    }
}

fn label(
    text: &str,
    bounds: Rect,
    h_align: HAlign,
    font_size: f32,
    color: Option<[u8; 3]>,
) -> LabelInfo<'_> {
    LabelInfo {
        text,
        bounds,
        h_align,
        v_align: VAlign::Center,
        overflow: Overflow::Ellipsis,
        padding: 0.0,
        font_size_override: Some(font_size),
        color_override: color,
        font_family_override: None,
    }
}

fn quad(
    rect: Rect,
    color: [f32; 4],
    border_color: [f32; 4],
    border_width: f32,
    radius: f32,
) -> QuadInstance {
    QuadInstance {
        rect: [rect.x, rect.y, rect.width, rect.height],
        color,
        color_bottom: color,
        border_color,
        border_width,
        border_radius: radius,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    }
}

fn card_quad(card: Rect) -> QuadInstance {
    QuadInstance {
        rect: [card.x, card.y, card.width, card.height],
        color: [0.19, 0.19, 0.24, 1.0],
        color_bottom: [0.13, 0.13, 0.17, 1.0],
        border_color: [0.43, 0.43, 0.53, 0.9],
        border_width: 1.5,
        border_radius: 16.0,
        shadow_offset: [0.0, 8.0],
        shadow_color: [0.0, 0.0, 0.0, 0.58],
        shadow_blur: 18.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    }
}

fn input_quad(rect: Rect, focused: bool) -> QuadInstance {
    QuadInstance {
        rect: [rect.x, rect.y, rect.width, rect.height],
        color: [0.09, 0.09, 0.12, 1.0],
        color_bottom: [0.07, 0.07, 0.10, 1.0],
        border_color: if focused {
            [0.45, 0.62, 0.95, 1.0]
        } else {
            [0.31, 0.31, 0.39, 0.9]
        },
        border_width: if focused { 2.0 } else { 1.0 },
        border_radius: 8.0,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    }
}

fn render_button<'a>(
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    rect: Rect,
    text: &'a str,
    primary: bool,
    enabled: bool,
    focused: bool,
) {
    let (top, bottom) = if !enabled {
        ([0.16, 0.16, 0.19, 1.0], [0.13, 0.13, 0.16, 1.0])
    } else if primary {
        ([0.36, 0.49, 0.86, 1.0], [0.27, 0.37, 0.73, 1.0])
    } else {
        ([0.28, 0.28, 0.34, 1.0], [0.21, 0.21, 0.26, 1.0])
    };
    quads.push(QuadInstance {
        rect: [rect.x, rect.y, rect.width, rect.height],
        color: top,
        color_bottom: bottom,
        border_color: if focused {
            [0.55, 0.70, 1.0, 1.0]
        } else {
            [0.50, 0.50, 0.60, if enabled { 0.7 } else { 0.25 }]
        },
        border_width: if focused { 2.0 } else { 1.0 },
        border_radius: 9.0,
        shadow_offset: [0.0, 3.0],
        shadow_color: [0.0, 0.0, 0.0, if enabled { 0.28 } else { 0.0 }],
        shadow_blur: 6.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    });
    labels.push(label(
        text,
        rect,
        HAlign::Center,
        13.0,
        Some(if enabled {
            [238, 238, 246]
        } else {
            [105, 105, 120]
        }),
    ));
}

fn field_metrics() -> text_input::TextInputMetrics {
    text_input::TextInputMetrics::left(FIELD_FONT_SIZE, FIELD_PADDING_X)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(status: ServerStatus) -> ServerInfo {
        ServerInfo {
            ip: "127.0.0.1".into(),
            port: 9050,
            endpoint: "127.0.0.1:9050".into(),
            name: "Local".into(),
            motd: String::new(),
            online: 0,
            max_slots: 0,
            status,
            players_text: String::new(),
        }
    }

    #[test]
    fn room_actions_only_enable_for_online_servers() {
        let mut modal = ServerBrowserModal {
            servers: vec![server(ServerStatus::Offline)],
            selected: Some(0),
            scroll_offset: 0.0,
            keyboard_focus: 0,
        };
        assert!(!modal.selected_online());
        modal.servers[0].status = ServerStatus::Online;
        assert!(modal.selected_online());
    }

    #[test]
    fn add_server_requires_a_valid_address_and_port() {
        let mut modal = AddServerModal::new();
        assert!(!modal.can_add());
        modal.ip = "localhost".into();
        assert!(modal.can_add());
        modal.port = "invalid".into();
        assert!(!modal.can_add());
    }
}
