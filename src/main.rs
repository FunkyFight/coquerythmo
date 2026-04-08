mod command;
mod config;
mod constants;
mod export;
mod graphics;
mod network;
mod observer;
mod packet;
mod rythmo_cpu_renderer;
mod rythmo_gpu_renderer;
mod syllable;
mod video_export;
mod i18n;
mod project;
mod rythmo_line;
mod state;
mod ui;
mod update;
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
            let mut dialog = rfd::FileDialog::new()
                .set_title(i18n::t("picker.video.title"))
                .add_filter("Video", &["mp4", "mov", "avi", "mkv", "webm"]);
            // Start in the last used directory, or the user's home
            if let Some(ref prev) = state.project_path {
                if let Some(parent) = prev.parent() {
                    dialog = dialog.set_directory(parent);
                }
            } else if let Some(home) = dirs::home_dir() {
                dialog = dialog.set_directory(home);
            }
            let file = dialog.pick_file();
            if let Some(path) = file {
                state.load_video(&path);
            }
        }
        UiAction::ExportProject => {
            use export::{JsonExporter, ProjectExporter};
            let exporter = JsonExporter;
            let file = rfd::FileDialog::new()
                .set_title(i18n::t("picker.export.title"))
                .add_filter(exporter.description(), &[exporter.extension()])
                .save_file();
            if let Some(path) = file {
                let fps = state.fps();
                if let Err(e) = exporter.export(&state.project, fps, &path) {
                    log::error!("Export failed: {e}");
                } else {
                    state.project_path = Some(path);
                    state.dirty = false;
                }
            }
        }
        UiAction::ImportProject => {
            use export::{JsonImporter, ProjectImporter};
            let file = rfd::FileDialog::new()
                .set_title(i18n::t("picker.import.title"))
                .add_filter("Bande rythmo JSON", &["json"])
                .pick_file();
            if let Some(path) = file {
                let importer = JsonImporter;
                match importer.import(&path) {
                    Ok(data) => {
                        let fps = state.fps();
                        data.apply_to_project(&mut state.project, fps);
                        state.project_path = Some(path.clone());
                        // Save to recent projects if video is loaded
                        if let Some(video) = state.video_path() {
                            config::add_recent_project(video, path);
                            state.rebuild_topbar_for_network();
                        }
                    }
                    Err(e) => log::error!("Import failed: {e}"),
                }
            }
        }
        UiAction::QuickSave => {
            use export::{JsonExporter, ProjectExporter};
            if let Some(path) = &state.project_path {
                let path = path.clone();
                let fps = state.fps();
                if let Err(e) = JsonExporter.export(&state.project, fps, &path) {
                    log::error!("Quick save failed: {e}");
                } else {
                    log::info!("Quick saved to {}", path.display());
                    state.dirty = false;
                }
            } else {
                // No path yet — fall back to save dialog
                let exporter = JsonExporter;
                let file = rfd::FileDialog::new()
                    .set_title(i18n::t("picker.export.title"))
                    .add_filter(exporter.description(), &[exporter.extension()])
                    .save_file();
                if let Some(path) = file {
                    let fps = state.fps();
                    if let Err(e) = exporter.export(&state.project, fps, &path) {
                        log::error!("Export failed: {e}");
                    } else {
                        state.project_path = Some(path);
                        state.dirty = false;
                    }
                }
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
            let line_id = state.create_line(frame, y_slot);
            state.start_editing_line(line_id);
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
        UiAction::OpenExportModal => {
            if state.video_path().is_some() {
                state.open_export_modal();
            } else {
                log::warn!("No video loaded — cannot export MP4");
            }
        }
        UiAction::StartExport { filename, fps } => {
            if let Some(source) = state.video_path() {
                let source_fps = state.fps();
                let project_snap = state.project.snapshot();
                let progress = std::sync::Arc::new(
                    std::sync::atomic::AtomicU32::new(0.0_f32.to_bits())
                );
                let progress_for_ui = progress.clone();

                let title = i18n::t("picker.export_mp4.title").to_string();
                let default_filename = if filename.ends_with(".mp4") {
                    filename
                } else {
                    format!("{}.mp4", filename)
                };
                std::thread::spawn(move || {
                    let file = rfd::FileDialog::new()
                        .set_title(&title)
                        .set_file_name(&default_filename)
                        .add_filter("MP4 Video", &["mp4"])
                        .save_file();
                    if let Some(output) = file {
                        // Signal that export is starting (progress overlay appears now)
                        progress.store(0.01_f32.to_bits(), std::sync::atomic::Ordering::Relaxed);
                        let p = progress.clone();
                        let result = video_export::export_mp4(
                            &project_snap, &source, &output, fps, source_fps,
                            move |v| {
                                p.store(v.to_bits(), std::sync::atomic::Ordering::Relaxed);
                            },
                        );
                        if let Err(e) = result {
                            log::error!("MP4 export failed: {e}");
                        }
                    }
                    progress.store(2.0_f32.to_bits(), std::sync::atomic::Ordering::Relaxed);
                });
                // Set progress tracker — overlay only visible once progress > 0
                state.set_export_progress(Some(progress_for_ui));
            } else {
                log::warn!("No video loaded — cannot export MP4");
            }
        }
        UiAction::OpenDropdown(dropdown) => {
            state.open_toolbar_dropdown(dropdown);
        }
        UiAction::DeleteSelected => {
            state.delete_selected();
        }
        UiAction::MoveMarker { index, frame } => {
            state.move_marker(index, frame);
        }
        UiAction::CancelExport => {
            state.set_export_progress(None);
        }
        UiAction::StopEditing => {
            state.broadcast_finalize();
        }
        UiAction::OpenRecentProject { video_path, br_path } => {
            use export::{JsonImporter, ProjectImporter};
            if video_path.exists() && br_path.exists() {
                state.load_video(&video_path);
                let importer = JsonImporter;
                match importer.import(&br_path) {
                    Ok(data) => {
                        let fps = state.fps();
                        data.apply_to_project(&mut state.project, fps);
                        state.project_path = Some(br_path.clone());
                        config::add_recent_project(video_path, br_path);
                        state.rebuild_topbar_for_network();
                        log::info!("Loaded recent project");
                    }
                    Err(e) => log::error!("Failed to load recent BR: {e}"),
                }
            } else {
                log::warn!("Recent project files missing, skipping");
            }
        }
        UiAction::ToggleSyllableMode => {
            state.toggle_syllable_mode();
        }
        UiAction::SetSyllableRatios { line_id, ratios } => {
            if let Some(line) = state.project.get_line_mut(line_id) {
                line.syllable_ratios = ratios;
            }
        }
        UiAction::OpenConnectModal { join } => {
            state.open_connect_modal(join);
        }
        UiAction::NetworkConnect { ip, port, password, username, room_code } => {
            // Save last used connection settings
            {
                let mut cfg = config::get().clone();
                cfg.network.server_ip = ip.clone();
                cfg.network.server_port = port;
                cfg.network.password = password.clone();
                cfg.network.username = username.clone();
                cfg.save();
            }
            let first_packet = if let Some(code) = room_code {
                packet::Packet::JoinRoom { code, username }
            } else {
                packet::Packet::CreateRoom { username }
            };
            state.network.connect_and_send(&ip, port, &password, first_packet);
        }
        UiAction::NetworkDisconnect => {
            state.network.disconnect();
            state.rebuild_topbar_for_network();
        }
        UiAction::OpenSettings => {
            state.open_settings_modal();
        }
        UiAction::SaveSettings { lang, rythmo_font, scroll_speed } => {
            crate::config::save_settings(lang, rythmo_font, scroll_speed);
            state.close_settings_modal();
        }
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

/// Parse an ICO file and return a winit Icon (RGBA pixels).
/// Picks the largest image entry, renders it via resvg's tiny-skia PNG decoder if PNG,
/// or falls back to raw BMP parsing.
fn parse_ico_to_winit_icon(ico_data: &[u8]) -> Option<winit::window::Icon> {
    if ico_data.len() < 6 { return None; }
    let count = u16::from_le_bytes([ico_data[4], ico_data[5]]) as usize;
    if count == 0 { return None; }

    // Find the largest entry
    let mut best_idx = 0;
    let mut best_size = 0u32;
    for i in 0..count {
        let off = 6 + i * 16;
        if off + 16 > ico_data.len() { break; }
        let w = if ico_data[off] == 0 { 256 } else { ico_data[off] as u32 };
        let h = if ico_data[off + 1] == 0 { 256 } else { ico_data[off + 1] as u32 };
        if w * h > best_size {
            best_size = w * h;
            best_idx = i;
        }
    }

    let entry_off = 6 + best_idx * 16;
    let img_size = u32::from_le_bytes([ico_data[entry_off+8], ico_data[entry_off+9], ico_data[entry_off+10], ico_data[entry_off+11]]) as usize;
    let img_offset = u32::from_le_bytes([ico_data[entry_off+12], ico_data[entry_off+13], ico_data[entry_off+14], ico_data[entry_off+15]]) as usize;

    if img_offset + img_size > ico_data.len() { return None; }
    let img_data = &ico_data[img_offset..img_offset + img_size];

    // Check if it's PNG (starts with PNG signature)
    if img_data.len() > 8 && img_data[0..4] == [0x89, 0x50, 0x4E, 0x47] {
        // Use resvg's tiny-skia to decode PNG
        let pixmap = resvg::tiny_skia::Pixmap::decode_png(img_data).ok()?;
        let w = pixmap.width();
        let h = pixmap.height();
        // Convert premultiplied RGBA to straight RGBA
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for pixel in pixmap.data().chunks_exact(4) {
            let a = pixel[3] as f32 / 255.0;
            if a > 0.0 {
                rgba.push((pixel[0] as f32 / a).min(255.0) as u8);
                rgba.push((pixel[1] as f32 / a).min(255.0) as u8);
                rgba.push((pixel[2] as f32 / a).min(255.0) as u8);
            } else {
                rgba.push(0); rgba.push(0); rgba.push(0);
            }
            rgba.push(pixel[3]);
        }
        winit::window::Icon::from_rgba(rgba, w, h).ok()
    } else {
        None // BMP entries not supported, skip
    }
}

#[cfg(target_os = "windows")]
fn clipboard_paste() -> Option<String> {
    use std::ptr;
    extern "system" {
        fn OpenClipboard(hwnd: *mut std::ffi::c_void) -> i32;
        fn CloseClipboard() -> i32;
        fn GetClipboardData(format: u32) -> *mut std::ffi::c_void;
        fn GlobalLock(hmem: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
        fn GlobalUnlock(hmem: *mut std::ffi::c_void) -> i32;
    }
    unsafe {
        if OpenClipboard(ptr::null_mut()) == 0 { return None; }
        let handle = GetClipboardData(13); // CF_UNICODETEXT
        if handle.is_null() { CloseClipboard(); return None; }
        let data = GlobalLock(handle) as *const u16;
        if data.is_null() { CloseClipboard(); return None; }
        let mut len = 0;
        while *data.add(len) != 0 { len += 1; }
        let text = String::from_utf16_lossy(std::slice::from_raw_parts(data, len));
        GlobalUnlock(handle);
        CloseClipboard();
        if text.is_empty() { None } else { Some(text) }
    }
}

#[cfg(not(target_os = "windows"))]
fn clipboard_paste() -> Option<String> { None }

fn main() {
    env_logger::init();
    config::init();
    i18n::init(&config::get().lang);

    // Check for updates (blocks briefly on network, shows dialog if update available)
    if update::check() {
        // Updater was launched, exit so it can replace our files
        return;
    }

    let cfg = config::get().clone();

    let event_loop = EventLoop::new().expect("Failed to create event loop");

    let window_icon = {
        let ico_data = include_bytes!("icons/app.ico");
        parse_ico_to_winit_icon(ico_data)
    };

    let window = Arc::new(
        WindowBuilder::new()
            .with_title(&cfg.window.title)
            .with_inner_size(LogicalSize::new(cfg.window.width, cfg.window.height))
            .with_window_icon(window_icon)
            .build(&event_loop)
            .expect("Failed to create window"),
    );

    let mut state = pollster::block_on(State::new(window.clone()));
    let mut cursor_pos = (0.0_f32, 0.0_f32);
    let mut last_click_time = Instant::now();
    let mut ctrl_held = false;
    let mut shift_held = false;

    event_loop
        .run(move |event, elwt| match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => elwt.exit(),
                WindowEvent::Resized(physical_size) => {
                    state.resize(physical_size);
                }
                WindowEvent::ModifiersChanged(modifiers) => {
                    ctrl_held = modifiers.state().control_key();
                    shift_held = modifiers.state().shift_key();
                    state.set_ctrl_held(ctrl_held);
                    state.request_redraw();
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
                            if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c == "v") {
                                // Ctrl+V — paste from clipboard
                                if let Some(text) = clipboard_paste() {
                                    dispatch(UiEvent::KeyInput { text }, &mut state, elwt);
                                }
                            } else if matches!(event.logical_key, Key::Named(NamedKey::ArrowLeft)) {
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
                                if !ctrl_held {
                                    dispatch(UiEvent::KeyInput { text: ch.to_string() }, &mut state, elwt);
                                }
                            }
                        } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c == "s") {
                            handle_action(UiAction::QuickSave, &mut state);
                            state.request_redraw();
                        } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c == "z") {
                            if event.repeat || !event.state.is_pressed() { /* skip */ } else {
                                state.undo();
                                state.request_redraw();
                            }
                        } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c == "Z") {
                            // CTRL+SHIFT+Z = redo (capital Z)
                            state.redo();
                            state.request_redraw();
                        } else if matches!(event.logical_key, Key::Named(NamedKey::Delete)) {
                            dispatch(UiEvent::Delete, &mut state, elwt);
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
                        x: cursor_pos.0, y: cursor_pos.1, delta: scroll_delta, fast: shift_held,
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
                            // Broadcast coalesced command on drag end
                            state.broadcast_finalize();
                        }
                    }
                }
                WindowEvent::MouseInput {
                    state: ref button_state,
                    button: MouseButton::Middle,
                    ..
                } => {
                    match button_state {
                        ElementState::Pressed => {
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
                WindowEvent::RedrawRequested => {
                    state.render();
                    state.request_redraw();
                }
                _ => {}
            },
            _ => {}
        })
        .expect("Event loop error");
}
