mod command;
mod config;
mod constants;
mod export;
mod graphics;
mod i18n;
mod network;
mod observer;
mod packet;
mod project;
mod rythmo_cpu_renderer;
mod rythmo_gpu_renderer;
mod rythmo_line;
mod state;
mod syllable;
mod ui;
mod update;
mod vector_text;
mod video;
mod video_export;
mod video_proxy;

use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::dpi::LogicalSize;
use winit::event::{ElementState, Event, MouseButton, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop, EventLoopWindowTarget};
use winit::keyboard::{Key, NamedKey};
use winit::window::WindowBuilder;

use state::State;
use ui::widget::{EventResponse, UiAction, UiEvent};

fn new_project_reset_and_pick_video(state: &mut State, elwt: &EventLoopWindowTarget<()>) {
    state.project.reset();
    state.project_path = None;
    state.dirty = false;
    state.clear_history();
    handle_action(UiAction::AddVideo, state, elwt);
}

fn handle_action(action: UiAction, state: &mut State, elwt: &EventLoopWindowTarget<()>) -> bool {
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
                    state.show_toast(i18n::t("toast.saved"), 3.0);
                    state.reload_linked_proxy();
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
                        state.reload_linked_proxy();
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
        UiAction::ImportCappelaProject => {
            let file = rfd::FileDialog::new()
                .set_title(i18n::t("picker.import.cappela.title"))
                .add_filter("Cappela DETX", &["detx"])
                .pick_file();
            if let Some(path) = file {
                let fps = state.fps();
                match export::import_cappela(&path, fps) {
                    Ok(data) => {
                        data.apply_to_project(&mut state.project, fps);
                        // On ne sauvegarde pas le path puisqu'on a importé un format qu'on va sauvegarder en .json
                        state.project_path = None;
                        state.dirty = true;
                    }
                    Err(e) => log::error!("Cappela import failed: {e}"),
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
                    state.show_toast(i18n::t("toast.saved"), 3.0);
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
                        state.show_toast(i18n::t("toast.saved"), 3.0);
                        state.reload_linked_proxy();
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
        UiAction::ToggleMute => {
            state.toggle_mute();
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
        UiAction::SeekAbsolute(frame) => {
            state.seek_absolute(frame);
        }
        UiAction::SeekToNextBoucle { direction } => {
            state.seek_to_next_boucle(direction);
        }
        UiAction::CreateLine { frame, y_slot } => {
            let line_id = state.create_line(frame, y_slot);
            state.start_editing_line(line_id);
        }
        UiAction::ResizeLine {
            id,
            start_frame,
            duration_frames,
        } => {
            state.resize_line(id, start_frame, duration_frames);
        }
        UiAction::MoveLine {
            id,
            start_frame,
            y_slot,
        } => {
            state.move_line(id, start_frame, y_slot);
        }
        UiAction::MoveLines { moves } => {
            state.move_lines(moves);
        }
        UiAction::UpdateLineText { id, text } => {
            state.update_line_text(id, text);
        }
        UiAction::SetCharacter {
            line_id,
            name,
            color,
        } => {
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
        UiAction::OpenProxyModal => {
            if state.video_path().is_none() {
                log::warn!("No video loaded — cannot create proxy");
            } else if state.project_path.is_none() {
                state.show_toast(i18n::t("toast.proxy_requires_saved_br"), 5.0);
            } else {
                state.open_proxy_modal();
            }
        }
        UiAction::CreateProxy { width, height, crf } => {
            let Some(source) = state.video_path() else {
                log::warn!("No video loaded — cannot create proxy");
                return false;
            };
            let Some(br_path) = state.project_path.clone() else {
                state.show_toast(i18n::t("toast.proxy_requires_saved_br"), 5.0);
                return false;
            };

            let progress =
                std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0.01_f32.to_bits()));
            let progress_for_ui = progress.clone();
            let (tx, rx) = std::sync::mpsc::channel();
            let source_for_job = source.clone();
            let br_for_job = br_path.clone();

            std::thread::spawn(move || {
                let p = progress.clone();
                let result = video_proxy::create_proxy(
                    &source_for_job,
                    &br_for_job,
                    width,
                    height,
                    crf,
                    move |v| {
                        p.store(v.to_bits(), std::sync::atomic::Ordering::Relaxed);
                    },
                );
                let _ = tx.send(result);
                progress.store(2.0_f32.to_bits(), std::sync::atomic::Ordering::Relaxed);
            });

            state.set_progress_label(i18n::t("progress.proxy"));
            state.set_export_progress(Some(progress_for_ui));
            state.watch_proxy_job(source, rx);
        }
        UiAction::StartExport {
            fps,
            br_scale,
            export_width,
            export_height,
            instrumental_audio_path,
        } => {
            if let Some(source) = state.video_path() {
                let source_fps = state.fps();
                let project_snap = state.project.snapshot();
                let progress =
                    std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0.0_f32.to_bits()));
                let progress_for_ui = progress.clone();

                let title = i18n::t("picker.export_mp4.title").to_string();
                std::thread::spawn(move || {
                    let file = rfd::FileDialog::new()
                        .set_title(&title)
                        .add_filter("MP4 Video", &["mp4"])
                        .save_file();
                    if let Some(output) = file {
                        // Signal that export is starting (progress overlay appears now)
                        progress.store(0.01_f32.to_bits(), std::sync::atomic::Ordering::Relaxed);
                        let p = progress.clone();
                        let result = video_export::export_mp4(
                            &project_snap,
                            &source,
                            &output,
                            fps,
                            source_fps,
                            br_scale,
                            export_width,
                            export_height,
                            instrumental_audio_path.as_deref(),
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
        UiAction::PickExportInstrumentalAudio => {
            let mut dialog = rfd::FileDialog::new()
                .set_title(i18n::t("picker.instrumental_audio.title"))
                .add_filter(
                    "Audio",
                    &["wav", "mp3", "m4a", "aac", "flac", "ogg", "opus"],
                );
            if let Some(source) = state.video_path() {
                if let Some(parent) = source.parent() {
                    dialog = dialog.set_directory(parent);
                }
            }
            if let Some(path) = dialog.pick_file() {
                state.set_export_instrumental_audio_path(path.to_string_lossy().into_owned());
            }
        }
        UiAction::OpenDropdown(dropdown) => {
            state.open_toolbar_dropdown(dropdown);
        }
        UiAction::OpenSecondaryDisplay => {
            if state.video_path().is_none() {
                log::warn!("No video loaded — cannot open secondary display");
            } else if state.has_secondary_display() {
                state.request_secondary_redraw();
            } else {
                match WindowBuilder::new()
                    .with_title(i18n::t("menu.tools.secondary_display"))
                    .with_inner_size(LogicalSize::new(1280.0, 720.0))
                    .with_window_icon(parse_ico_to_winit_icon(include_bytes!("icons/app.ico")))
                    .build(elwt)
                {
                    Ok(window) => state.open_secondary_display(Arc::new(window)),
                    Err(e) => log::error!("Failed to create secondary display window: {e}"),
                }
            }
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
        UiAction::OpenRecentProject {
            video_path,
            br_path,
        } => {
            use export::{JsonImporter, ProjectImporter};
            if video_path.exists() && br_path.exists() {
                state.project_path = Some(br_path.clone());
                state.load_video(&video_path);
                let importer = JsonImporter;
                match importer.import(&br_path) {
                    Ok(data) => {
                        let fps = state.fps();
                        data.apply_to_project(&mut state.project, fps);
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
        UiAction::OpenServerBrowser => {
            state.open_server_browser();
        }
        UiAction::OpenConnectModal { ip, port, join } => {
            state.open_connect_modal(&ip, port, join);
        }
        UiAction::OpenAddServerModal => {
            state.open_add_server_modal();
        }
        UiAction::AddServer { ip, port } => {
            config::add_server(ip, port);
            state.refresh_server_browser();
        }
        UiAction::RemoveServer(index) => {
            config::remove_server(index);
            state.refresh_server_browser();
        }
        UiAction::RefreshServers => {
            state.refresh_server_browser();
        }
        UiAction::NetworkConnect {
            ip,
            port,
            password,
            username,
            room_code,
        } => {
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
            state
                .network
                .connect_and_send(&ip, port, &password, first_packet);
        }
        UiAction::NetworkDisconnect => {
            state.network.disconnect();
            state.rebuild_topbar_for_network();
        }
        UiAction::RestoreBackup => {
            if state.restore_backup() {
                state.show_toast(i18n::t("toast.backup_restored"), 5.0);
            } else {
                state.show_toast(i18n::t("toast.no_backup"), 4.0);
            }
        }
        UiAction::OpenSettings => {
            state.open_settings_modal();
        }
        UiAction::SaveSettings {
            lang,
            rythmo_font,
            scroll_speed,
        } => {
            crate::config::save_settings(lang, rythmo_font, scroll_speed);
            state.close_settings_modal();
        }
        UiAction::NewProject => {
            if state.dirty && !state.project.is_empty() {
                state.open_save_prompt();
            } else {
                new_project_reset_and_pick_video(state, elwt);
            }
        }
        UiAction::NewProjectSave => {
            handle_action(UiAction::QuickSave, state, elwt);
            new_project_reset_and_pick_video(state, elwt);
        }
        UiAction::NewProjectDiscard => {
            new_project_reset_and_pick_video(state, elwt);
        }
        UiAction::EnterStudioMode => {
            state.enter_studio_mode();
        }
        UiAction::ShowStudioWarning => {
            state.open_studio_warning();
        }
        UiAction::AddNote => {
            state.start_editing_note_selected();
        }
        UiAction::UpdateLineNote { line_id, note } => {
            state.update_line_note(line_id, note);
        }
        UiAction::SetClipboard(text) => {
            clipboard_set(&text);
        }
        UiAction::SetClipboardAndUpdateLineText {
            clipboard,
            id,
            text,
        } => {
            clipboard_set(&clipboard);
            state.update_line_text(id, text);
        }
        UiAction::SetClipboardAndUpdateCharacterName {
            clipboard,
            line_id,
            name,
        } => {
            clipboard_set(&clipboard);
            state.update_character_name(line_id, name);
        }
        UiAction::SetClipboardAndUpdateLineNote {
            clipboard,
            line_id,
            note,
        } => {
            clipboard_set(&clipboard);
            state.update_line_note(line_id, note);
        }
        UiAction::CopySelectedLine => {
            state.copy_selected_line();
        }
        UiAction::CutSelectedLine => {
            state.cut_selected_line();
        }
        UiAction::PasteLine => {
            state.paste_line();
        }
    }
    false
}

fn dispatch(
    ui_event: UiEvent,
    state: &mut State,
    elwt: &winit::event_loop::EventLoopWindowTarget<()>,
) {
    if let EventResponse::Action(action) = state.handle_ui_event(&ui_event) {
        if handle_action(action, state, elwt) {
            elwt.exit();
        }
    }
    state.request_redraw();
}

/// Parse an ICO file and return a winit Icon (RGBA pixels).
/// Picks the largest image entry, renders it via resvg's tiny-skia PNG decoder if PNG,
/// or falls back to raw BMP parsing.
fn parse_ico_to_winit_icon(ico_data: &[u8]) -> Option<winit::window::Icon> {
    if ico_data.len() < 6 {
        return None;
    }
    let count = u16::from_le_bytes([ico_data[4], ico_data[5]]) as usize;
    if count == 0 {
        return None;
    }

    // Find the largest entry
    let mut best_idx = 0;
    let mut best_size = 0u32;
    for i in 0..count {
        let off = 6 + i * 16;
        if off + 16 > ico_data.len() {
            break;
        }
        let w = if ico_data[off] == 0 {
            256
        } else {
            ico_data[off] as u32
        };
        let h = if ico_data[off + 1] == 0 {
            256
        } else {
            ico_data[off + 1] as u32
        };
        if w * h > best_size {
            best_size = w * h;
            best_idx = i;
        }
    }

    let entry_off = 6 + best_idx * 16;
    let img_size = u32::from_le_bytes([
        ico_data[entry_off + 8],
        ico_data[entry_off + 9],
        ico_data[entry_off + 10],
        ico_data[entry_off + 11],
    ]) as usize;
    let img_offset = u32::from_le_bytes([
        ico_data[entry_off + 12],
        ico_data[entry_off + 13],
        ico_data[entry_off + 14],
        ico_data[entry_off + 15],
    ]) as usize;

    if img_offset + img_size > ico_data.len() {
        return None;
    }
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
                rgba.push(0);
                rgba.push(0);
                rgba.push(0);
            }
            rgba.push(pixel[3]);
        }
        winit::window::Icon::from_rgba(rgba, w, h).ok()
    } else {
        None // BMP entries not supported, skip
    }
}

#[cfg(target_os = "windows")]
fn clipboard_set(text: &str) {
    use std::ptr;
    extern "system" {
        fn OpenClipboard(hwnd: *mut std::ffi::c_void) -> i32;
        fn CloseClipboard() -> i32;
        fn EmptyClipboard() -> i32;
        fn SetClipboardData(format: u32, hmem: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
        fn GlobalAlloc(flags: u32, bytes: usize) -> *mut std::ffi::c_void;
        fn GlobalLock(hmem: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
        fn GlobalUnlock(hmem: *mut std::ffi::c_void) -> i32;
    }
    const GMEM_MOVEABLE: u32 = 0x0002;
    unsafe {
        if OpenClipboard(ptr::null_mut()) == 0 {
            return;
        }
        EmptyClipboard();
        let mut wide: Vec<u16> = text.encode_utf16().collect();
        wide.push(0);
        let bytes = wide.len() * std::mem::size_of::<u16>();
        let handle = GlobalAlloc(GMEM_MOVEABLE, bytes);
        if !handle.is_null() {
            let data = GlobalLock(handle) as *mut u16;
            if !data.is_null() {
                ptr::copy_nonoverlapping(wide.as_ptr(), data, wide.len());
                GlobalUnlock(handle);
                SetClipboardData(13, handle);
            }
        }
        CloseClipboard();
    }
}

#[cfg(not(target_os = "windows"))]
fn clipboard_set(_text: &str) {}

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
        if OpenClipboard(ptr::null_mut()) == 0 {
            return None;
        }
        let handle = GetClipboardData(13); // CF_UNICODETEXT
        if handle.is_null() {
            CloseClipboard();
            return None;
        }
        let data = GlobalLock(handle) as *const u16;
        if data.is_null() {
            CloseClipboard();
            return None;
        }
        let mut len = 0;
        while *data.add(len) != 0 {
            len += 1;
        }
        let text = String::from_utf16_lossy(std::slice::from_raw_parts(data, len));
        GlobalUnlock(handle);
        CloseClipboard();
        if text.is_empty() {
            None
        } else {
            Some(text)
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn clipboard_paste() -> Option<String> {
    None
}

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
    state.show_toast(i18n::t("toast.welcome"), 10.0);
    let mut cursor_pos = (0.0_f32, 0.0_f32);
    let mut last_click_time = Instant::now();
    let mut ctrl_held = false;
    let mut shift_held = false;

    event_loop
        .run(move |event, elwt| {
            let secondary_video_running = state.has_secondary_display() && state.is_video_playing();
            if secondary_video_running {
                elwt.set_control_flow(ControlFlow::WaitUntil(
                    Instant::now() + Duration::from_millis(16),
                ));
            } else {
                elwt.set_control_flow(ControlFlow::Wait);
            }

            match event {
                Event::WindowEvent { window_id, event } => {
                if state.is_secondary_window(window_id) {
                    match event {
                        WindowEvent::CloseRequested => state.close_secondary_display(),
                        WindowEvent::KeyboardInput { event, .. } => {
                            if event.state == ElementState::Pressed {
                                if matches!(event.logical_key, Key::Named(NamedKey::Space)) {
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
                            state.request_secondary_redraw();
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
                }
                WindowEvent::ModifiersChanged(modifiers) => {
                    ctrl_held = modifiers.state().control_key();
                    shift_held = modifiers.state().shift_key();
                    state.set_ctrl_held(ctrl_held);
                    state.request_redraw();
                }
                WindowEvent::KeyboardInput { event, .. } => {
                    if event.state == ElementState::Pressed {
                        // F5: show studio warning if video is loaded
                        if matches!(event.logical_key, Key::Named(NamedKey::F5)) && state.video_path().is_some() {
                            handle_action(UiAction::ShowStudioWarning, &mut state, elwt);
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
                            Key::Named(NamedKey::Space) => Some(" "),
                            _ => None,
                        };

                        if state.is_editing_text() {
                            if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c == "a") {
                                // Ctrl+A — select all text
                                dispatch(UiEvent::SelectAll, &mut state, elwt);
                            } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c.eq_ignore_ascii_case("c")) {
                                dispatch(UiEvent::Copy, &mut state, elwt);
                            } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c.eq_ignore_ascii_case("x")) {
                                dispatch(UiEvent::Cut, &mut state, elwt);
                            } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c.eq_ignore_ascii_case("z")) {
                                dispatch(UiEvent::UndoTextEdit, &mut state, elwt);
                            } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c.eq_ignore_ascii_case("v")) {
                                // Ctrl+V — paste from clipboard
                                if let Some(text) = clipboard_paste() {
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
                            } else if let Some(t) = key_text {
                                dispatch(UiEvent::KeyInput { text: t.into() }, &mut state, elwt);
                            } else if let Key::Character(ch) = &event.logical_key {
                                if !ctrl_held {
                                    dispatch(UiEvent::KeyInput { text: ch.to_string() }, &mut state, elwt);
                                }
                            }
                        } else if state.is_studio_mode() {
                            // In studio mode: only Space (play/pause) is allowed
                            if matches!(event.logical_key, Key::Named(NamedKey::Space)) {
                                state.toggle_play_pause();
                                state.request_redraw();
                            }
                        } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c == "s") {
                            handle_action(UiAction::QuickSave, &mut state, elwt);
                            state.request_redraw();
                        } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c.eq_ignore_ascii_case("a")) {
                            dispatch(UiEvent::SelectAll, &mut state, elwt);
                        } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c.eq_ignore_ascii_case("c")) {
                            handle_action(UiAction::CopySelectedLine, &mut state, elwt);
                            state.request_redraw();
                        } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c.eq_ignore_ascii_case("x")) {
                            handle_action(UiAction::CutSelectedLine, &mut state, elwt);
                            state.request_redraw();
                        } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c.eq_ignore_ascii_case("v")) {
                            handle_action(UiAction::PasteLine, &mut state, elwt);
                            state.request_redraw();
                        } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c == "z") {
                            if event.repeat || !event.state.is_pressed() { /* skip */ } else {
                                state.undo();
                                state.request_redraw();
                            }
                        } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c == "n") {
                            handle_action(UiAction::NewProject, &mut state, elwt);
                            state.request_redraw();
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
                    if state.is_studio_mode() {
                        // In studio mode: scroll navigates between boucles
                        // Positive delta (scroll up) = forward (+1), negative delta (scroll down) = backward (-1)
                        let direction = if scroll_delta > 0.0 { 1 } else { -1 };
                        if ctrl_held {
                            // CTRL+SHIFT+scroll: jump to next/prev boucle
                            handle_action(UiAction::SeekToNextBoucle { direction }, &mut state, elwt);
                        } else {
                            // Regular scroll: seek by frames
                            let frame_delta = (scroll_delta.abs() * 10.0) as i32 * direction;
                            handle_action(UiAction::SeekRelative(frame_delta), &mut state, elwt);
                        }
                        state.request_redraw();
                    } else {
                        dispatch(UiEvent::Scroll {
                            x: cursor_pos.0, y: cursor_pos.1, delta: scroll_delta, fast: shift_held, ctrl: ctrl_held,
                        }, &mut state, elwt);
                    }
                }
                WindowEvent::CursorMoved { position, .. } => {
                    cursor_pos = (position.x as f32, position.y as f32);
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

                    if is_text_cursor {
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
                                let is_double = now.duration_since(last_click_time).as_millis() < 400;
                                last_click_time = now;

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
                    if !state.is_video_playing() {
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
                    }
                }
                _ => {}
                }
            }
                Event::AboutToWait => {
                if state.has_secondary_display() && state.is_video_playing() {
                    if let Some(window_id) = state.secondary_window_id() {
                        state.render_secondary_display(window_id);
                    }
                } else {
                    state.request_secondary_redraw();
                }
                state.request_redraw();
            }
            _ => {}
            }
        })
        .expect("Event loop error");
}
