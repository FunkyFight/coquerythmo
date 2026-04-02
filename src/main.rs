mod command;
mod config;
mod graphics;
mod observer;
mod i18n;
mod project;
mod rythmo_line;
mod state;
mod ui;
mod video;

use std::sync::Arc;
use std::time::Instant;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, Event, MouseButton, WindowEvent};
use winit::event_loop::EventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::WindowBuilder;

use state::State;
use ui::widget::{EventResponse, UiAction, UiEvent};

fn handle_action(action: UiAction, state: &mut State) -> bool {
    match action {
        UiAction::CloseApp => return true,
        UiAction::AddVideo => {
            let file = rfd::FileDialog::new()
                .set_title(i18n::t("picker.video.title"))
                .add_filter("Video", &["mp4", "mov", "avi", "mkv", "webm"])
                .pick_file();
            if let Some(path) = file {
                state.load_video(&path);
            }
        }
        UiAction::TogglePlayPause => {
            state.toggle_play_pause();
        }
        UiAction::SetVolume(vol) => {
            state.set_volume(vol);
        }
        UiAction::PrevFrame => {
            state.prev_frame();
        }
        UiAction::NextFrame => {
            state.next_frame();
        }
        UiAction::SeekRelative(delta) => {
            state.seek_relative(delta);
        }
        UiAction::CreateLine { frame, y_slot } => {
            state.create_line(frame, y_slot);
        }
        UiAction::ResizeLine { id, start_frame, duration_frames } => {
            state.resize_line(id, start_frame, duration_frames);
        }
        UiAction::MoveLine { id, start_frame, y_slot } => {
            state.move_line(id, start_frame, y_slot);
        }
        UiAction::UpdateLineText { id, text } => {
            state.update_line_text(id, text);
        }
        UiAction::SetCharacter { line_id, name, color } => {
            state.set_character(line_id, name, color);
        }
        UiAction::SetCharacterColor { line_id, color } => {
            state.set_character_color(line_id, color);
        }
        UiAction::UpdateCharacterName { line_id, name } => {
            state.update_character_name(line_id, name);
        }
        UiAction::FinalizeCharacter { line_id } => {
            state.finalize_character(line_id);
        }
        UiAction::AddMarker(kind) => {
            state.add_marker(kind);
        }
        UiAction::AddQuickLine { text } => {
            state.add_quick_line(text);
        }
        UiAction::OpenDropdown(dropdown) => {
            state.open_toolbar_dropdown(dropdown);
        }
        UiAction::StopEditing => {}
    }
    false
}

fn dispatch(ui_event: UiEvent, state: &mut State, elwt: &winit::event_loop::EventLoopWindowTarget<()>) {
    if let EventResponse::Action(action) = state.handle_ui_event(&ui_event) {
        if handle_action(action, state) {
            elwt.exit();
        }
    }
    state.request_redraw();
}

fn main() {
    env_logger::init();
    config::init();

    let cfg = config::get();

    let event_loop = EventLoop::new().unwrap();
    let window = Arc::new(
        WindowBuilder::new()
            .with_title(&cfg.window.title)
            .with_inner_size(LogicalSize::new(cfg.window.width, cfg.window.height))
            .build(&event_loop)
            .unwrap(),
    );

    let mut state = pollster::block_on(State::new(window.clone()));
    let mut cursor_pos = (0.0_f32, 0.0_f32);
    let mut last_click_time = Instant::now();
    let mut ctrl_held = false;

    event_loop
        .run(move |event, elwt| match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => elwt.exit(),
                WindowEvent::Resized(physical_size) => {
                    state.resize(physical_size);
                }
                WindowEvent::ModifiersChanged(modifiers) => {
                    ctrl_held = modifiers.state().control_key();
                }
                WindowEvent::KeyboardInput { event, .. } => {
                    if event.state == ElementState::Pressed {
                        let key_text = match &event.logical_key {
                            Key::Named(NamedKey::Escape) => Some("\x1b"),
                            Key::Named(NamedKey::Backspace) => Some("\x08"),
                            Key::Named(NamedKey::Enter) => Some("\r"),
                            Key::Named(NamedKey::Space) => Some(" "),
                            _ => None,
                        };

                        if state.is_editing_text() {
                            if matches!(event.logical_key, Key::Named(NamedKey::ArrowLeft)) {
                                dispatch(UiEvent::CursorLeft, &mut state, elwt);
                            } else if matches!(event.logical_key, Key::Named(NamedKey::ArrowRight)) {
                                dispatch(UiEvent::CursorRight, &mut state, elwt);
                            } else if matches!(event.logical_key, Key::Named(NamedKey::ArrowUp)) {
                                dispatch(UiEvent::CursorUp, &mut state, elwt);
                            } else if matches!(event.logical_key, Key::Named(NamedKey::ArrowDown)) {
                                dispatch(UiEvent::CursorDown, &mut state, elwt);
                            } else if let Some(t) = key_text {
                                dispatch(UiEvent::KeyInput { text: t.into() }, &mut state, elwt);
                            } else if let Key::Character(ch) = &event.logical_key {
                                dispatch(UiEvent::KeyInput { text: ch.to_string() }, &mut state, elwt);
                            }
                        } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c == "z") {
                            if event.repeat || !event.state.is_pressed() { /* skip */ } else {
                                state.undo();
                                state.request_redraw();
                            }
                        } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c == "Z") {
                            // CTRL+SHIFT+Z = redo (capital Z)
                            state.redo();
                            state.request_redraw();
                        } else if matches!(event.logical_key, Key::Named(NamedKey::Space)) {
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
                    dispatch(UiEvent::Scroll {
                        x: cursor_pos.0, y: cursor_pos.1, delta: scroll_delta,
                    }, &mut state, elwt);
                }
                WindowEvent::CursorMoved { position, .. } => {
                    cursor_pos = (position.x as f32, position.y as f32);
                    dispatch(UiEvent::MouseMove {
                        x: cursor_pos.0, y: cursor_pos.1,
                    }, &mut state, elwt);
                }
                WindowEvent::MouseInput {
                    state: ref button_state,
                    button: MouseButton::Left,
                    ..
                } => {
                    match button_state {
                        ElementState::Pressed => {
                            let now = Instant::now();
                            let is_double = now.duration_since(last_click_time).as_millis() < 400;
                            last_click_time = now;

                            if ctrl_held {
                                dispatch(UiEvent::CtrlClick {
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
                        }
                    }
                }
                WindowEvent::RedrawRequested => {
                    state.render();
                    state.request_redraw();
                }
                _ => {}
            },
            _ => {}
        })
        .unwrap();
}
