use super::bootstrap;
use super::dispatcher::{dispatch, CommandDispatcher};
use crate::config;
use crate::application::edit_service::EditExecutor;
use crate::i18n;
use crate::input::context::{InputContext, InputContextStack};
use crate::input::key::{InputWindow, KeyStroke, Modifiers};
use crate::input::router::existing_shortcuts;
use crate::platform;
use crate::state::State;
use crate::ui;
use crate::ui::primitives::{UiAction, UiEvent};
use crate::update;
use std::sync::Arc;
use std::time::Instant;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, Event, MouseButton, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy, EventLoopWindowTarget};
use winit::keyboard::{Key, NamedKey};

#[derive(Debug)]
pub(crate) enum AppEvent {
    WhatsNewFetched {
        version: String,
        result: Result<update::ReleaseInfo, String>,
    },
}

fn start_whats_new_fetch(version: String, proxy: EventLoopProxy<AppEvent>) {
    let tag = format!("v{version}");
    std::thread::spawn(move || {
        let result = update::fetch_release_by_tag(&tag);
        let _ = proxy.send_event(AppEvent::WhatsNewFetched { version, result });
    });
}

fn handle_whats_new_result(
    version: String,
    result: Result<update::ReleaseInfo, String>,
    state: &mut State,
) {
    match result {
        Ok(release) => {
            let expected_tag = format!("v{version}");
            if release.tag_name != expected_tag {
                log::warn!(
                    "Ignoring release notes for {}, expected {}",
                    release.tag_name,
                    expected_tag
                );
                return;
            }
            state.open_whats_new_modal(version.clone(), release.body);
            config::mark_whats_new_seen(&version);
        }
        Err(e) => {
            log::warn!("Could not fetch release notes for version {version}: {e}");
        }
    }
}

pub(crate) fn new_project_reset_and_pick_video(
    state: &mut State,
    elwt: &EventLoopWindowTarget<AppEvent>,
) {
    EditExecutor::reset(&mut state.project_session);
    CommandDispatcher::dispatch(UiAction::AddVideo, state, elwt);
}

fn is_space_key(key: &Key) -> bool {
    matches!(key, Key::Named(NamedKey::Space))
        || matches!(key, Key::Character(text) if text.as_str() == " ")
}
pub fn run() {
    let event_loop = EventLoopBuilder::<AppEvent>::with_user_event()
        .build()
        .expect("Failed to create event loop");
    let event_loop_proxy = event_loop.create_proxy();
    let shortcuts = existing_shortcuts();

    if bootstrap::initialize() {
        // Updater was launched, exit so it can replace our files
        return;
    }

    let cfg = config::get().clone();

    let window_icon = platform::app_icon();

    let window = Arc::new(
        platform::window_builder()
            .with_title(&cfg.window.title)
            .with_inner_size(LogicalSize::new(cfg.window.width, cfg.window.height))
            .with_window_icon(window_icon)
            .build(&event_loop)
            .expect("Failed to create window"),
    );

    let mut state = pollster::block_on(State::new(window.clone()));
    state.update_window_title();
    if config::should_show_whats_new(update::current_version()) {
        start_whats_new_fetch(update::current_version().to_string(), event_loop_proxy);
    }
    state.show_toast(i18n::t("toast.welcome"), 10.0);
    let mut cursor_pos = (0.0_f32, 0.0_f32);
    let mut last_click_time = None;
    let mut ctrl_held = false;
    let mut shift_held = false;

    event_loop
        .run(move |event, elwt| {
            match event {
                Event::UserEvent(AppEvent::WhatsNewFetched { version, result }) => {
                    handle_whats_new_result(version, result, &mut state);
                    state.request_redraw();
                }
                Event::WindowEvent { window_id, event } => {
                if state.is_secondary_window(window_id) {
                    match event {
                        WindowEvent::CloseRequested => state.close_secondary_display(),
                        WindowEvent::KeyboardInput { event, .. } => {
                            if event.state == ElementState::Pressed {
                                let modifiers = Modifiers::NONE;
                                let routed = KeyStroke::from_winit(
                                    &event,
                                    modifiers,
                                    InputWindow::Secondary,
                                )
                                .and_then(|stroke| {
                                    shortcuts
                                        .resolve(
                                            &stroke,
                                            &InputContextStack::new([InputContext::SecondaryWindow]),
                                        )
                                        .cloned()
                                });
                                if let Some(action) = routed {
                                    if CommandDispatcher::dispatch(action, &mut state, elwt) {
                                        elwt.exit();
                                    }
                                    state.request_secondary_redraw();
                                } else if is_space_key(&event.logical_key) {
                                    state.toggle_play_pause();
                                    state.request_redraw();
                                    state.request_secondary_redraw();
                                } else if matches!(event.logical_key, Key::Named(NamedKey::Escape)) {
                                    state.close_secondary_display();
                                }
                            }
                        }
                        WindowEvent::Resized(physical_size) => {
                            state.resize_secondary_display(window_id, physical_size);
                            state.request_secondary_redraw();
                        }
                        WindowEvent::RedrawRequested => {
                            state.render_secondary_display(window_id);
                        }
                        _ => {}
                    }
                    return;
                }

                if window_id != window.id() {
                    return;
                }

                match event {
                WindowEvent::CloseRequested => elwt.exit(),
                WindowEvent::Resized(physical_size) => {
                    state.resize(physical_size);
                    state.request_redraw();
                }
                WindowEvent::ScaleFactorChanged { .. } => {
                    state.resize(window.inner_size());
                    state.request_redraw();
                }
                WindowEvent::ModifiersChanged(modifiers) => {
                    ctrl_held = modifiers.state().control_key();
                    shift_held = modifiers.state().shift_key();
                    state.set_ctrl_held(ctrl_held);
                    state.request_redraw();
                }
                WindowEvent::KeyboardInput { event, .. } => {
                    if event.state == ElementState::Pressed {
                        let mut contexts = Vec::new();
                        if state.is_rythmo_text_editing() {
                            contexts.push(InputContext::TextEditing);
                        } else if !state.is_editing_text() {
                            if state.video_path().is_some() {
                                contexts.push(InputContext::VideoLoaded);
                            }
                            if state.is_studio_mode() {
                                contexts.push(InputContext::Studio);
                            } else {
                                contexts.push(InputContext::Workspace);
                                contexts.push(InputContext::Global);
                            }
                        }
                        let context_stack = InputContextStack::new(contexts);
                        let modifiers = Modifiers {
                            ctrl: ctrl_held,
                            shift: shift_held,
                            ..Modifiers::NONE
                        };
                        if let Some(stroke) = KeyStroke::from_winit(
                            &event,
                            modifiers,
                            InputWindow::Main,
                        ) {
                            if let Some(action) = shortcuts.resolve(&stroke, &context_stack).cloned() {
                                if CommandDispatcher::dispatch(action, &mut state, elwt) {
                                    elwt.exit();
                                }
                                state.request_redraw();
                                return;
                            }
                        }
                        // F5: show studio warning if video is loaded
                        if matches!(event.logical_key, Key::Named(NamedKey::F5)) && state.video_path().is_some() {
                            CommandDispatcher::dispatch(UiAction::ShowStudioWarning, &mut state, elwt);
                            state.request_redraw();
                            return;
                        }
                        // ESCAPE: exit studio mode if active
                        if matches!(event.logical_key, Key::Named(NamedKey::Escape)) && state.is_studio_mode() {
                            state.exit_studio_mode();
                            state.request_redraw();
                            return;
                        }

                        let key_text = match &event.logical_key {
                            Key::Named(NamedKey::Escape) => Some("\x1b"),
                            Key::Named(NamedKey::Backspace) => Some("\x08"),
                            Key::Named(NamedKey::Enter) => Some("\r"),
                            Key::Named(NamedKey::Tab) => Some("\t"),
                            _ => None,
                        };
                        let key_text = if is_space_key(&event.logical_key) {
                            Some(" ")
                        } else {
                            key_text
                        };

                        if state.is_studio_mode() {
                            // In studio mode: only Space (play/pause) is allowed
                            if is_space_key(&event.logical_key) {
                                state.toggle_play_pause();
                                state.request_redraw();
                            }
                        } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c.eq_ignore_ascii_case("k")) {
                            CommandDispatcher::dispatch(UiAction::SplitDialogue, &mut state, elwt);
                            state.request_redraw();
                        } else if state.is_editing_text() {
                            if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c == "a") {
                                // Ctrl+A â€” select all text
                                dispatch(UiEvent::SelectAll, &mut state, elwt);
                            } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c.eq_ignore_ascii_case("c")) {
                                dispatch(UiEvent::Copy, &mut state, elwt);
                            } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c.eq_ignore_ascii_case("x")) {
                                dispatch(UiEvent::Cut, &mut state, elwt);
                            } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c.eq_ignore_ascii_case("z")) {
                                dispatch(UiEvent::UndoTextEdit, &mut state, elwt);
                            } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c.eq_ignore_ascii_case("v")) {
                                // Ctrl+V â€” paste from clipboard
                                if let Some(text) = platform::clipboard_paste() {
                                    dispatch(UiEvent::KeyInput { text }, &mut state, elwt);
                                }
                            } else if matches!(event.logical_key, Key::Named(NamedKey::ArrowLeft)) {
                                if shift_held {
                                    dispatch(UiEvent::ShiftCursorLeft, &mut state, elwt);
                                } else {
                                    dispatch(UiEvent::CursorLeft, &mut state, elwt);
                                }
                            } else if matches!(event.logical_key, Key::Named(NamedKey::ArrowRight)) {
                                if shift_held {
                                    dispatch(UiEvent::ShiftCursorRight, &mut state, elwt);
                                } else {
                                    dispatch(UiEvent::CursorRight, &mut state, elwt);
                                }
                            } else if matches!(event.logical_key, Key::Named(NamedKey::ArrowUp)) {
                                dispatch(UiEvent::CursorUp, &mut state, elwt);
                            } else if matches!(event.logical_key, Key::Named(NamedKey::ArrowDown)) {
                                dispatch(UiEvent::CursorDown, &mut state, elwt);
                            } else if matches!(event.logical_key, Key::Named(NamedKey::Delete)) {
                                dispatch(UiEvent::KeyInput { text: "\x7f".into() }, &mut state, elwt);
                            } else if let Some(t) = key_text {
                                dispatch(UiEvent::KeyInput { text: t.into() }, &mut state, elwt);
                            } else if let Key::Character(ch) = &event.logical_key {
                                if !ctrl_held {
                                    dispatch(UiEvent::KeyInput { text: ch.to_string() }, &mut state, elwt);
                                }
                            }
                        } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c == "s") {
                            CommandDispatcher::dispatch(UiAction::QuickSave, &mut state, elwt);
                            state.request_redraw();
                        } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c.eq_ignore_ascii_case("a")) {
                            dispatch(UiEvent::SelectAll, &mut state, elwt);
                        } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c.eq_ignore_ascii_case("c")) {
                            CommandDispatcher::dispatch(UiAction::CopySelectedLine, &mut state, elwt);
                            state.request_redraw();
                        } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c.eq_ignore_ascii_case("x")) {
                            CommandDispatcher::dispatch(UiAction::CutSelectedLine, &mut state, elwt);
                            state.request_redraw();
                        } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c.eq_ignore_ascii_case("v")) {
                            CommandDispatcher::dispatch(UiAction::PasteLine, &mut state, elwt);
                            state.request_redraw();
                        } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c == "z") {
                            if event.repeat || !event.state.is_pressed() { /* skip */ } else {
                                state.undo();
                                state.request_redraw();
                            }
                        } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c == "n") {
                            CommandDispatcher::dispatch(UiAction::NewProject, &mut state, elwt);
                            state.request_redraw();
                        } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c == "Z") {
                            // CTRL+SHIFT+Z = redo (capital Z)
                            state.redo();
                            state.request_redraw();
                        } else if matches!(event.logical_key, Key::Named(NamedKey::Delete)) {
                            dispatch(UiEvent::Delete, &mut state, elwt);
                        } else if matches!(event.logical_key, Key::Named(NamedKey::Tab)) {
                            CommandDispatcher::dispatch(UiAction::ToggleActiveAudio, &mut state, elwt);
                            state.request_redraw();
                        } else if is_space_key(&event.logical_key) {
                            state.toggle_play_pause();
                            state.request_redraw();
                        }
                    }
                }
                WindowEvent::MouseWheel { delta, .. } => {
                    let scroll_delta = match delta {
                        winit::event::MouseScrollDelta::LineDelta(_, y) => y,
                        winit::event::MouseScrollDelta::PixelDelta(pos) => pos.y as f32 / 40.0,
                    };
                    if state.is_studio_mode() {
                        // In studio mode: scroll navigates between boucles
                        // Positive delta (scroll up) = forward (+1), negative delta (scroll down) = backward (-1)
                        let direction = if scroll_delta > 0.0 { 1 } else { -1 };
                        if ctrl_held {
                            // CTRL+SHIFT+scroll: jump to next/prev boucle
                            CommandDispatcher::dispatch(UiAction::SeekToNextBoucle { direction }, &mut state, elwt);
                        } else {
                            // Regular scroll: seek by frames
                            let frame_delta = ui::shell::scroll_delta_to_frames(scroll_delta, 10.0);
                            if frame_delta != 0 {
                                CommandDispatcher::dispatch(UiAction::SeekRelative(frame_delta), &mut state, elwt);
                            }
                        }
                        state.request_redraw();
                    } else {
                        dispatch(UiEvent::Scroll {
                            x: cursor_pos.0, y: cursor_pos.1, delta: scroll_delta, fast: shift_held, ctrl: ctrl_held,
                        }, &mut state, elwt);
                    }
                }
                WindowEvent::CursorMoved { position, .. } => {
                    cursor_pos = state.window_to_ui_position(position.x as f32, position.y as f32);
                    // Always dispatch mouse move (needed for panning in studio mode)
                    dispatch(UiEvent::MouseMove {
                        x: cursor_pos.0, y: cursor_pos.1,
                    }, &mut state, elwt);

                    // Update cursor icon if hover over active text
                    let is_text_cursor = {
                        let mut res = false;
                        if state.is_editing_text() {
                            if let Some(h) = state.hovered_line() {
                                if state.editing_line() == Some(h) {
                                    res = true;
                                }
                            }
                        }
                        res
                    };

                    let resize_cursor = state.hovering_resize_handle() || state.dragging_resize_handle();

                    if resize_cursor {
                        window.set_cursor_icon(winit::window::CursorIcon::NsResize);
                    } else if is_text_cursor {
                        window.set_cursor_icon(winit::window::CursorIcon::Text);
                    } else {
                        window.set_cursor_icon(winit::window::CursorIcon::Default);
                    }
                }
                WindowEvent::MouseInput {
                    state: ref button_state,
                    button: MouseButton::Left,
                    ..
                } => {
                    if !state.is_studio_mode() {
                        match button_state {
                            ElementState::Pressed => {
                                let now = Instant::now();
                                let is_double = last_click_time
                                    .map(|last| now.duration_since(last).as_millis() < 400)
                                    .unwrap_or(false);
                                last_click_time = Some(now);

                                if ctrl_held {
                                    dispatch(UiEvent::CtrlClick {
                                        x: cursor_pos.0, y: cursor_pos.1,
                                    }, &mut state, elwt);
                                } else if shift_held {
                                    dispatch(UiEvent::ShiftMousePress {
                                        x: cursor_pos.0, y: cursor_pos.1,
                                    }, &mut state, elwt);
                                } else if is_double {
                                    dispatch(UiEvent::DoubleClick {
                                        x: cursor_pos.0, y: cursor_pos.1,
                                    }, &mut state, elwt);
                                } else {
                                    dispatch(UiEvent::MousePress {
                                        x: cursor_pos.0, y: cursor_pos.1,
                                    }, &mut state, elwt);
                                }
                            }
                            ElementState::Released => {
                                dispatch(UiEvent::MouseRelease {
                                    x: cursor_pos.0, y: cursor_pos.1,
                                }, &mut state, elwt);
                                // Broadcast coalesced command on drag end
                                state.broadcast_finalize();
                            }
                        }
                    }
                }
                WindowEvent::MouseInput {
                    state: ref button_state,
                    button: MouseButton::Middle,
                    ..
                } => {
                    // Allow middle click panning in both editor and studio modes
                    match button_state {
                        ElementState::Pressed => {
                            if state.is_studio_mode() {
                                state.begin_timeline_pan(cursor_pos.0);
                            }
                            dispatch(UiEvent::MiddlePress {
                                x: cursor_pos.0, y: cursor_pos.1,
                            }, &mut state, elwt);
                        }
                        ElementState::Released => {
                            dispatch(UiEvent::MiddleRelease {
                                x: cursor_pos.0, y: cursor_pos.1,
                            }, &mut state, elwt);
                        }
                    }
                }
                WindowEvent::MouseInput {
                    state: ref button_state,
                    button: MouseButton::Right,
                    ..
                } => {
                    if !state.is_studio_mode() && matches!(button_state, ElementState::Pressed) {
                        dispatch(UiEvent::ContextMenu {
                            x: cursor_pos.0,
                            y: cursor_pos.1,
                        }, &mut state, elwt);
                    }
                }
                WindowEvent::RedrawRequested => {
                    state.render();
                    if state.secondary_needs_continuous_redraw() {
                        state.request_secondary_redraw();
                    }
                }
                WindowEvent::DroppedFile(path) => {
                    let ext = path.extension()
                        .and_then(|e| e.to_str())
                        .map(|e| e.to_lowercase())
                        .unwrap_or_default();
                    if ["mp4", "mov", "avi", "mkv", "webm"].contains(&ext.as_str()) {
                        state.load_video(&path);
                        state.request_redraw();
                    }
                }
                _ => {}
                }
            }
            Event::AboutToWait => {
                let changed = state.tick_background();
                let needs_continuous = state.needs_continuous_redraw();
                if changed || state.needs_redraw_now() {
                    state.request_redraw();
                    if state.has_secondary_display() {
                        state.request_secondary_redraw();
                    }
                }

                if needs_continuous {
                    // Use Poll for continuous rendering at monitor refresh rate (60fps+)
                    // The OS/window system will throttle to vsync automatically
                    elwt.set_control_flow(ControlFlow::Poll);
                } else if let Some(deadline) = state.next_wake_deadline() {
                    let now = Instant::now();
                    elwt.set_control_flow(ControlFlow::WaitUntil(deadline.max(now)));
                } else {
                    elwt.set_control_flow(ControlFlow::Wait);
                }
            }
            _ => {}
            }
        })
        .expect("Event loop error");
}
