use super::bootstrap;
use super::dispatcher::{dispatch, CommandDispatcher};
use crate::application::edit_service::EditExecutor;
use crate::application::workspace_service::WorkspaceId;
use crate::config;
use crate::i18n;
use crate::input::context::{InputContext, InputContextStack};
use crate::input::key::{InputWindow, KeyStroke, Modifiers};
use crate::input::router::existing_shortcuts;
use crate::platform;
use crate::state::State;
use crate::ui::primitives::{UiAction, UiEvent};
use crate::update;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, Event, MouseButton, TouchPhase, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy, EventLoopWindowTarget};
use winit::keyboard::{Key, KeyCode as WinitKeyCode, NamedKey, PhysicalKey};

#[derive(Debug)]
pub(crate) enum AppEvent {
    WhatsNewFetched {
        version: String,
        result: Result<update::ReleaseInfo, String>,
    },
    #[cfg(target_os = "windows")]
    AccessKitAction(accesskit_winit::ActionRequestEvent),
}

#[cfg(target_os = "windows")]
impl From<accesskit_winit::ActionRequestEvent> for AppEvent {
    fn from(event: accesskit_winit::ActionRequestEvent) -> Self {
        Self::AccessKitAction(event)
    }
}

fn start_whats_new_fetch(version: String, proxy: EventLoopProxy<AppEvent>) {
    let tag = format!("v{version}");
    std::thread::spawn(move || {
        let fetch = if config::dev_mode() {
            #[cfg(debug_assertions)]
            {
                Ok(update::dev_release(&version))
            }
            #[cfg(not(debug_assertions))]
            {
                update::fetch_release_by_tag(&tag)
            }
        } else {
            update::fetch_release_by_tag(&tag)
        };
        let result = fetch.map(|mut release| {
            if let Some(url) = release.youtube_url.clone() {
                match update::fetch_youtube_thumbnail(&url) {
                    Ok(thumbnail) => release.thumbnail = Some(thumbnail),
                    Err(error) => log::warn!("Could not fetch YouTube thumbnail: {error}"),
                }
            }
            release
        });
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
            state.open_whats_new_modal(
                version.clone(),
                release.body,
                release.youtube_url,
                release.thumbnail,
            );
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
    state.clear_video_for_new_project();
    EditExecutor::reset(&mut state.project_session);
    state.recording_runtime = crate::recording_runtime::RecordingRuntime::new();
    state.ui_shell.ui.reset_recording_workspace();
    crate::vector_text::clear_project_font();
    state.render.ui_renderer.clear_text_cache();
    CommandDispatcher::dispatch(UiAction::AddVideo, state, elwt);
}

pub(crate) fn close_project_reset(state: &mut State) {
    state.clear_video_for_new_project();
    EditExecutor::reset(&mut state.project_session);
    state.recording_runtime = crate::recording_runtime::RecordingRuntime::new();
    state.ui_shell.ui.reset_recording_workspace();
    crate::vector_text::clear_project_font();
    state.render.ui_renderer.clear_text_cache();
}

fn is_space_key(key: &Key) -> bool {
    matches!(key, Key::Named(NamedKey::Space))
        || matches!(key, Key::Character(text) if text.as_str() == " ")
}

fn is_control_key(key: PhysicalKey) -> bool {
    matches!(
        key,
        PhysicalKey::Code(WinitKeyCode::ControlLeft | WinitKeyCode::ControlRight)
    )
}

fn is_shift_key(key: PhysicalKey) -> bool {
    matches!(
        key,
        PhysicalKey::Code(WinitKeyCode::ShiftLeft | WinitKeyCode::ShiftRight)
    )
}

fn dispatch_key_action(
    action: UiAction,
    _event: &winit::event::KeyEvent,
    _modifiers: Modifiers,
    _input_window: InputWindow,
    state: &mut State,
    elwt: &EventLoopWindowTarget<AppEvent>,
) -> bool {
    CommandDispatcher::dispatch_shortcut(action, state, elwt)
}

fn dispatch_key_action_with_explicit_announcement(
    action: UiAction,
    event: &winit::event::KeyEvent,
    modifiers: Modifiers,
    input_window: InputWindow,
    state: &mut State,
    elwt: &EventLoopWindowTarget<AppEvent>,
) -> bool {
    dispatch_key_action(action, event, modifiers, input_window, state, elwt)
}

fn announce_key_action(
    action: &UiAction,
    event: &winit::event::KeyEvent,
    modifiers: Modifiers,
    input_window: InputWindow,
    state: &State,
) {
    let _ = (event, modifiers, input_window);
    CommandDispatcher::announce_shortcut(action, state);
}

fn announce_key_chord(
    _event: &winit::event::KeyEvent,
    _modifiers: Modifiers,
    _input_window: InputWindow,
    _state: &State,
) {
    // Low-level key chords have no semantic action name. Keep them silent
    // instead of reading the physical keys as a fallback.
}

fn announce_named_text_command(command: crate::application::command::TextCommand, state: &State) {
    CommandDispatcher::announce_shortcut(&UiAction::Text(command), state);
}

pub fn run(startup_path: Option<PathBuf>) {
    if bootstrap::initialize() {
        // Updater was launched, exit so it can replace our files
        return;
    }

    let event_loop = EventLoopBuilder::<AppEvent>::with_user_event()
        .build()
        .expect("Failed to create event loop");
    let event_loop_proxy = event_loop.create_proxy();
    let shortcuts = existing_shortcuts();

    let cfg = config::get().clone();

    let window_icon = platform::app_icon();

    let window = Arc::new(
        platform::window_builder()
            .with_title(&cfg.window.title)
            .with_inner_size(LogicalSize::new(cfg.window.width, cfg.window.height))
            .with_window_icon(window_icon)
            .with_visible(!cfg!(target_os = "windows"))
            .build(&event_loop)
            .expect("Failed to create window"),
    );

    #[cfg(target_os = "windows")]
    let windows_accessibility =
        crate::accessibility::WindowsAccessibilityAdapter::new(&window, event_loop.create_proxy());
    #[cfg(target_os = "windows")]
    window.set_visible(true);

    #[cfg(target_os = "windows")]
    let (accessibility_sender, accessibility_receiver) = std::sync::mpsc::channel();
    #[cfg(target_os = "windows")]
    let accessibility_sender = Some(accessibility_sender);
    #[cfg(not(target_os = "windows"))]
    let accessibility_sender = None;

    let mut state = pollster::block_on(State::new(window.clone(), accessibility_sender));
    state.update_window_title();
    if let Some(path) = startup_path {
        state.start_br_import(path);
    }
    if config::should_show_whats_new(update::current_version()) {
        start_whats_new_fetch(update::current_version().to_string(), event_loop_proxy);
    }
    state.show_toast(i18n::t("toast.welcome"), 10.0);
    let mut cursor_pos = (0.0_f32, 0.0_f32);
    let mut last_click_time = None;
    let mut ctrl_held = false;
    let mut shift_held = false;
    let mut shift_used_as_modifier = false;
    let mut keyboard_modifiers = Modifiers::NONE;
    let mut cursor_icon = winit::window::CursorIcon::Default;
    let mut last_pointer_dispatch: Option<Instant> = None;
    let mut last_dispatched_cursor_pos = cursor_pos;

    event_loop
        .run(move |event, elwt| {
            match event {
                Event::UserEvent(AppEvent::WhatsNewFetched { version, result }) => {
                    handle_whats_new_result(version, result, &mut state);
                    state.request_redraw();
                }
                #[cfg(target_os = "windows")]
                Event::UserEvent(AppEvent::AccessKitAction(event)) => {
                    log::trace!("AccessKit action request: {:?}", event.request);
                }
                Event::WindowEvent { window_id, event } => {
                #[cfg(target_os = "windows")]
                if window_id == window.id() {
                    windows_accessibility.process_event(&window, &event);
                }
                if state.is_secondary_window(window_id) {
                    match event {
                        WindowEvent::CloseRequested => state.close_secondary_display(),
                        WindowEvent::KeyboardInput { event, .. } => {
                            if event.state == ElementState::Pressed {
                                if is_control_key(event.physical_key) {
                                    state.narration.stop_for_control();
                                    return;
                                }
                                if is_shift_key(event.physical_key) && !event.repeat {
                                    state.resume_narration();
                                    return;
                                }
                                let modifiers = Modifiers::NONE;
                                let mut handled = false;
                                if let Some(stroke) = KeyStroke::from_winit(
                                    &event,
                                    modifiers,
                                    InputWindow::Secondary,
                                ) {
                                    if let Some(action) = shortcuts
                                        .resolve(
                                            &stroke,
                                            &InputContextStack::new([InputContext::SecondaryWindow]),
                                        )
                                        .cloned()
                                    {
                                        if CommandDispatcher::dispatch_shortcut(
                                            action,
                                            &mut state,
                                            elwt,
                                        ) {
                                            elwt.exit();
                                        }
                                        handled = true;
                                    }
                                    state.request_secondary_redraw();
                                }
                                if !handled && is_space_key(&event.logical_key) {
                                    dispatch_key_action(
                                        UiAction::TogglePlayPause,
                                        &event,
                                        modifiers,
                                        InputWindow::Secondary,
                                        &mut state,
                                        elwt,
                                    );
                                    state.request_redraw();
                                    state.request_secondary_redraw();
                                } else if !handled
                                    && matches!(event.logical_key, Key::Named(NamedKey::Escape))
                                {
                                    dispatch_key_action(
                                        UiAction::CloseSecondaryDisplay,
                                        &event,
                                        modifiers,
                                        InputWindow::Secondary,
                                        &mut state,
                                        elwt,
                                    );
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
                WindowEvent::CloseRequested => {
                    if state.is_project_save_in_progress() {
                        state.show_toast(i18n::t("toast.close_blocked_saving"), 5.0);
                        state.request_redraw();
                    } else {
                        if CommandDispatcher::dispatch(UiAction::ExitApplication, &mut state, elwt) {
                            elwt.exit();
                        } else {
                            state.request_redraw();
                        }
                    }
                }
                WindowEvent::Resized(physical_size) => {
                    state.resize(physical_size);
                    state.request_redraw();
                }
                WindowEvent::ScaleFactorChanged { .. } => {
                    state.resize(window.inner_size());
                    state.render.update_refresh_interval();
                    state.request_redraw();
                }
                WindowEvent::Moved(_) => {
                    state.render.update_refresh_interval();
                }
                WindowEvent::ModifiersChanged(modifiers) => {
                    keyboard_modifiers = Modifiers::from_winit(modifiers.state());
                    ctrl_held = modifiers.state().control_key();
                    shift_held = modifiers.state().shift_key();
                    state.set_ctrl_held(ctrl_held);
                    state.request_redraw();
                }
                WindowEvent::KeyboardInput { event, .. } => {
                    if event.state == ElementState::Pressed && is_control_key(event.physical_key) {
                        // Ctrl is an always-available speech cut-off, including
                        // while a modal, progress dialog or text field owns input.
                        state.narration.stop_for_control();
                        state.request_redraw();
                        return;
                    }
                    if event.state == ElementState::Pressed
                        && is_shift_key(event.physical_key)
                        && !event.repeat
                    {
                        shift_used_as_modifier = false;
                        state.resume_narration();
                        state.request_redraw();
                        return;
                    }
                    if event.state == ElementState::Pressed
                        && shift_held
                        && !is_shift_key(event.physical_key)
                    {
                        shift_used_as_modifier = true;
                    }
                    if event.state == ElementState::Released && is_shift_key(event.physical_key) {
                        if !shift_used_as_modifier
                            && !state.is_editing_text()
                            && !state.captures_modal_input()
                            && state.has_selected_detection()
                        {
                            dispatch_key_action(
                                UiAction::ToggleSelectedSyncAffinity,
                                &event,
                                keyboard_modifiers,
                                InputWindow::Main,
                                &mut state,
                                elwt,
                            );
                            state.request_redraw();
                        }
                        shift_used_as_modifier = false;
                        return;
                    }
                    // Release-driven commands (notably continuous Q/D panning)
                    // must be routed even though text input only consumes presses.
                    if event.state == ElementState::Released {
                        if !state.is_editing_text()
                            && !state.captures_modal_input()
                            && state.active_workspace() == WorkspaceId::Rythmo
                        {
                            if let Some(stroke) = KeyStroke::from_winit(
                                &event,
                                keyboard_modifiers,
                                InputWindow::Main,
                            ) {
                                if let Some(action) = shortcuts
                                    .resolve(
                                        &stroke,
                                        &InputContextStack::new([InputContext::Workspace]),
                                    )
                                    .cloned()
                                {
                                    // Release bindings finish an already-announced
                                    // shortcut (for example Q/D panning); do not
                                    // speak the same key chord a second time.
                                    CommandDispatcher::dispatch(action, &mut state, elwt);
                                    state.request_redraw();
                                }
                            }
                        }
                        return;
                    }
                    if event.state == ElementState::Pressed {
                        // Accessibility is deliberately above progress dialogs,
                        // modal traps and text editing so speech can always stop.
                        if let Some(stroke) = KeyStroke::from_winit(
                            &event,
                            keyboard_modifiers,
                            InputWindow::Main,
                        ) {
                            if let Some(action) = shortcuts
                                .resolve(
                                    &stroke,
                                    &InputContextStack::new([InputContext::Accessibility]),
                                )
                                .cloned()
                            {
                                CommandDispatcher::dispatch_shortcut(
                                    action,
                                    &mut state,
                                    elwt,
                                );
                                state.request_redraw();
                                return;
                            }
                        }
                        if state.ui_shell.ui.has_active_progress() {
                            if matches!(event.logical_key, Key::Named(NamedKey::Escape)) {
                                dispatch_key_action(
                                    UiAction::CancelExport,
                                    &event,
                                    keyboard_modifiers,
                                    InputWindow::Main,
                                    &mut state,
                                    elwt,
                                );
                                state.request_redraw();
                            }
                            return;
                        }
                        if state.ui_shell.ui.loading_project.is_some() {
                            return;
                        }
                        // Keep frame-by-frame line nudging independent from
                        // focus routing. In particular, Ctrl is also used to
                        // control narration, so these chords must reach the
                        // workspace even when another shell control owns
                        // keyboard focus.
                        if ctrl_held
                            && shift_held
                            && !state.is_editing_text()
                            && !state.captures_modal_input()
                            && matches!(
                                event.logical_key,
                                Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowRight)
                            )
                        {
                            let delta_frames = if matches!(
                                event.logical_key,
                                Key::Named(NamedKey::ArrowLeft)
                            ) {
                                -1
                            } else {
                                1
                            };
                            let action = if state.has_selected_detection() {
                                UiAction::NudgeSelectedDetection {
                                    delta_ticks: crate::detection::MediaTick::from_frame(
                                        delta_frames,
                                    )
                                    .raw(),
                                }
                            } else {
                                UiAction::NudgeSelectedLines { delta_frames }
                            };
                            dispatch_key_action(
                                action,
                                &event,
                                keyboard_modifiers,
                                InputWindow::Main,
                                &mut state,
                                elwt,
                            );
                            state.request_redraw();
                            return;
                        }
                        // A rythmo text editor owns every arrow key.  This
                        // guard is intentionally before the workspace
                        // shortcut table so Shift+Up/Down cannot leak into
                        // the volume command while a line or character is
                        // being edited.
                        if state.is_rythmo_text_editing() {
                            let text_navigation = match &event.logical_key {
                                Key::Named(NamedKey::ArrowLeft) => Some(if ctrl_held && shift_held {
                                    UiEvent::SelectWordLeft
                                } else if shift_held {
                                    UiEvent::ShiftCursorLeft
                                } else {
                                    UiEvent::CursorLeft
                                }),
                                Key::Named(NamedKey::ArrowRight) => Some(if ctrl_held && shift_held {
                                    UiEvent::SelectWordRight
                                } else if shift_held {
                                    UiEvent::ShiftCursorRight
                                } else {
                                    UiEvent::CursorRight
                                }),
                                Key::Named(NamedKey::ArrowUp) => Some(if ctrl_held && shift_held {
                                    UiEvent::SelectAll
                                } else {
                                    UiEvent::CursorUp
                                }),
                                Key::Named(NamedKey::ArrowDown) => Some(UiEvent::CursorDown),
                                Key::Named(NamedKey::Home) => Some(UiEvent::Home),
                                Key::Named(NamedKey::End) => Some(UiEvent::End),
                                _ => None,
                            };
                            if let Some(text_navigation) = text_navigation {
                                if shift_held
                                    || matches!(
                                        event.logical_key,
                                        Key::Named(NamedKey::Home | NamedKey::End)
                                    )
                                {
                                    announce_key_chord(
                                        &event,
                                        keyboard_modifiers,
                                        InputWindow::Main,
                                        &state,
                                    );
                                }
                                dispatch(text_navigation, &mut state, elwt);
                                return;
                            }
                        }
                        // Shift+Left/Right traverses every line in timeline
                        // order, even when accessibility focus is currently on
                        // a toolbar/control and no line is selected yet.
                        if shift_held
                            && !ctrl_held
                            && !state.is_editing_text()
                            && !state.captures_modal_input()
                            && matches!(
                                event.logical_key,
                                Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowRight)
                            )
                        {
                            let direction = if matches!(
                                event.logical_key,
                                Key::Named(NamedKey::ArrowLeft)
                            ) {
                                -1
                            } else {
                                1
                            };
                            dispatch_key_action(
                                UiAction::NavigateLines { direction },
                                &event,
                                keyboard_modifiers,
                                InputWindow::Main,
                                &mut state,
                                elwt,
                            );
                            state.request_redraw();
                            return;
                        }
                        if matches!(event.logical_key, Key::Named(NamedKey::Escape))
                            && !state.captures_modal_input()
                            && !state.is_editing_text()
                            && state.active_workspace() == WorkspaceId::Rythmo
                            && !state.side_panel_open()
                            && state.has_selected_lines()
                        {
                            dispatch_key_action(
                                UiAction::ClearLineSelection,
                                &event,
                                keyboard_modifiers,
                                InputWindow::Main,
                                &mut state,
                                elwt,
                            );
                            state.request_redraw();
                            return;
                        }
                        // Modal controls do not depend on the shell focus tree:
                        // they always receive Escape, arrows, Enter and Space.
                        if state.captures_modal_input() {
                            let modal_event = match &event.logical_key {
                                Key::Named(NamedKey::Escape) => Some(UiEvent::KeyInput {
                                    text: "\x1b".into(),
                                }),
                                Key::Named(NamedKey::ArrowLeft) => Some(
                                    if ctrl_held && shift_held {
                                        UiEvent::SelectWordLeft
                                    } else if shift_held {
                                        UiEvent::ShiftCursorLeft
                                    } else {
                                        UiEvent::CursorLeft
                                    },
                                ),
                                Key::Named(NamedKey::ArrowRight) => Some(
                                    if ctrl_held && shift_held {
                                        UiEvent::SelectWordRight
                                    } else if shift_held {
                                        UiEvent::ShiftCursorRight
                                    } else {
                                        UiEvent::CursorRight
                                    },
                                ),
                                Key::Named(NamedKey::ArrowUp) => Some(UiEvent::CursorUp),
                                Key::Named(NamedKey::ArrowDown) => Some(UiEvent::CursorDown),
                                Key::Named(NamedKey::Home) => Some(UiEvent::Home),
                                Key::Named(NamedKey::End) => Some(UiEvent::End),
                                Key::Named(NamedKey::Enter) => Some(UiEvent::KeyInput {
                                    text: "\r".into(),
                                }),
                                key if is_space_key(key) => Some(UiEvent::KeyInput {
                                    text: " ".into(),
                                }),
                                _ => None,
                            };
                            if let Some(modal_event) = modal_event {
                                dispatch(modal_event, &mut state, elwt);
                                state.request_redraw();
                                return;
                            }
                        }
                        // Escape leaves the shell's keyboard-focus mode. Keep
                        // modal and text-editor Escape handling ahead of this
                        // so their own cancel/close behavior remains intact.
                        if matches!(event.logical_key, Key::Named(NamedKey::Escape))
                            && !state.captures_modal_input()
                            && !state.is_editing_text()
                            && !state.side_panel_open()
                            && state.has_keyboard_focus()
                        {
                            dispatch(
                                UiEvent::KeyInput {
                                    text: "\x1b".into(),
                                },
                                &mut state,
                                elwt,
                            );
                            return;
                        }
                        // The translation manager is global, but never stack it over an
                        // existing modal whose event routing would remain underneath it.
                        if ctrl_held
                            && !event.repeat
                            && !state.captures_modal_input()
                            && matches!(&event.logical_key, Key::Character(c) if c.eq_ignore_ascii_case("l"))
                        {
                            dispatch_key_action(
                                UiAction::OpenLanguages,
                                &event,
                                keyboard_modifiers,
                                InputWindow::Main,
                                &mut state,
                                elwt,
                            );
                            state.request_redraw();
                            return;
                        }
                        if state.side_panel_open() && ctrl_held {
                            if state.is_editing_text()
                                && matches!(&event.logical_key, Key::Character(c) if c.eq_ignore_ascii_case("v"))
                            {
                                announce_named_text_command(
                                    crate::application::command::TextCommand::Paste,
                                    &state,
                                );
                                if let Some(text) = platform::clipboard_paste() {
                                    dispatch(UiEvent::KeyInput { text }, &mut state, elwt);
                                }
                                state.request_redraw();
                                return;
                            }
                            let panel_event = match &event.logical_key {
                                Key::Character(c) if c.eq_ignore_ascii_case("a") => {
                                    Some(UiEvent::SelectAll)
                                }
                                Key::Character(c) if c.eq_ignore_ascii_case("c") => {
                                    Some(UiEvent::Copy)
                                }
                                Key::Character(c) if c.eq_ignore_ascii_case("x") => {
                                    Some(UiEvent::Cut)
                                }
                                Key::Character(c)
                                    if c.eq_ignore_ascii_case("z")
                                        && !shift_held
                                        && state.is_editing_text() =>
                                {
                                    Some(UiEvent::UndoTextEdit)
                                }
                                _ => None,
                            };
                            if let Some(panel_event) = panel_event {
                                let command = match &panel_event {
                                    UiEvent::SelectAll => {
                                        Some(crate::application::command::TextCommand::SelectAll)
                                    }
                                    UiEvent::Copy => {
                                        Some(crate::application::command::TextCommand::Copy)
                                    }
                                    UiEvent::Cut => {
                                        Some(crate::application::command::TextCommand::Cut)
                                    }
                                    UiEvent::UndoTextEdit => {
                                        Some(crate::application::command::TextCommand::Undo)
                                    }
                                    _ => None,
                                };
                                if let Some(command) = command {
                                    announce_named_text_command(command, &state);
                                }
                                dispatch(panel_event, &mut state, elwt);
                                state.request_redraw();
                                return;
                            }
                        }
                        if state.side_panel_open() && !ctrl_held {
                            let panel_event = match &event.logical_key {
                                Key::Named(NamedKey::Escape) => Some(UiEvent::KeyInput {
                                    text: "\x1b".to_string(),
                                }),
                                Key::Named(NamedKey::Enter) => Some(UiEvent::Activate),
                                key if is_space_key(key) => Some(UiEvent::KeyInput {
                                    text: " ".to_string(),
                                }),
                                Key::Named(NamedKey::ArrowLeft) if !shift_held => {
                                    Some(UiEvent::CursorLeft)
                                }
                                Key::Named(NamedKey::ArrowRight) if !shift_held => {
                                    Some(UiEvent::CursorRight)
                                }
                                Key::Named(NamedKey::ArrowUp) => Some(UiEvent::CursorUp),
                                Key::Named(NamedKey::ArrowDown) => Some(UiEvent::CursorDown),
                                Key::Named(NamedKey::Home) => Some(UiEvent::Home),
                                Key::Named(NamedKey::End) => Some(UiEvent::End),
                                Key::Named(NamedKey::PageUp) => Some(UiEvent::PageUp),
                                Key::Named(NamedKey::PageDown) => Some(UiEvent::PageDown),
                                Key::Named(NamedKey::Delete) => Some(UiEvent::Delete),
                                _ => None,
                            };
                            if let Some(panel_event) = panel_event {
                                dispatch(panel_event, &mut state, elwt);
                                state.request_redraw();
                                return;
                            }
                        }
                        if matches!(event.logical_key, Key::Named(NamedKey::Escape))
                            && !state.captures_modal_input()
                            && !state.is_editing_text()
                            && state.has_selected_detection()
                        {
                            state.focus_detection_parent_line();
                            state.request_redraw();
                            return;
                        }
                        if !ctrl_held
                            && !shift_held
                            && keyboard_modifiers.alt
                            && !event.repeat
                            && !state.captures_modal_input()
                            && !state.is_editing_text()
                            && state.active_workspace() == WorkspaceId::Rythmo
                            && state.rythmo_detection_hovered()
                            && matches!(&event.logical_key, Key::Character(c) if c.eq_ignore_ascii_case("d"))
                        {
                            state.open_detection_palette_from_hover();
                            state.request_redraw();
                            return;
                        }

                        let mut contexts = Vec::new();
                        if state.captures_modal_input() {
                            contexts.push(InputContext::Modal);
                        } else if state.is_rythmo_text_editing() {
                            contexts.push(InputContext::TextEditing);
                        } else if state.has_keyboard_focus() {
                            contexts.push(InputContext::MainWindow);
                            contexts.push(InputContext::Global);
                        } else if !state.is_editing_text() {
                            match state.active_workspace() {
                                WorkspaceId::Rythmo => contexts.push(InputContext::Workspace),
                                WorkspaceId::Recording => contexts.push(InputContext::Recording),
                            }
                            contexts.push(InputContext::Global);
                        }
                        let context_stack = InputContextStack::new(contexts);
                        if let Some(stroke) = KeyStroke::from_winit(
                            &event,
                            keyboard_modifiers,
                            InputWindow::Main,
                        ) {
                            if let Some(action) = shortcuts.resolve(&stroke, &context_stack).cloned() {
                                if CommandDispatcher::dispatch_shortcut(
                                    action,
                                    &mut state,
                                    elwt,
                                ) {
                                    elwt.exit();
                                }
                                state.request_redraw();
                                return;
                            }
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

                        // A line editor may coexist with a previously focused
                        // syllable separator. Plain Space belongs to the text
                        // editor in that state; do not let the stale focus
                        // activate (and advance to) the separator instead.
                        // Modified Space chords have already been resolved by
                        // the shortcut router above.
                        if state.is_rythmo_text_editing()
                            && !ctrl_held
                            && !keyboard_modifiers.alt
                            && is_space_key(&event.logical_key)
                        {
                            dispatch(
                                UiEvent::KeyInput {
                                    text: " ".to_string(),
                                },
                                &mut state,
                                elwt,
                            );
                            state.request_redraw();
                            return;
                        }

                        if matches!(event.logical_key, Key::Named(NamedKey::Tab)) {
                            dispatch(
                                if shift_held {
                                    UiEvent::FocusPrevious
                                } else {
                                    UiEvent::FocusNext
                                },
                                &mut state,
                                elwt,
                            );
                            return;
                        }
                        if keyboard_modifiers.alt
                            && matches!(event.logical_key, Key::Named(NamedKey::ArrowLeft))
                        {
                            if shift_held && state.has_selected_detection() {
                                dispatch_key_action(
                                    UiAction::NudgeSelectedSyncAnchor {
                                        delta_graphemes: -1,
                                    },
                                    &event,
                                    keyboard_modifiers,
                                    InputWindow::Main,
                                    &mut state,
                                    elwt,
                                );
                                state.request_redraw();
                                return;
                            }
                            announce_key_chord(
                                &event,
                                keyboard_modifiers,
                                InputWindow::Main,
                                &state,
                            );
                            dispatch(UiEvent::AltCursorLeft, &mut state, elwt);
                            return;
                        }
                        if keyboard_modifiers.alt
                            && matches!(event.logical_key, Key::Named(NamedKey::ArrowRight))
                        {
                            if shift_held && state.has_selected_detection() {
                                dispatch_key_action(
                                    UiAction::NudgeSelectedSyncAnchor {
                                        delta_graphemes: 1,
                                    },
                                    &event,
                                    keyboard_modifiers,
                                    InputWindow::Main,
                                    &mut state,
                                    elwt,
                                );
                                state.request_redraw();
                                return;
                            }
                            announce_key_chord(
                                &event,
                                keyboard_modifiers,
                                InputWindow::Main,
                                &state,
                            );
                            dispatch(UiEvent::AltCursorRight, &mut state, elwt);
                            return;
                        }
                        if shift_held
                            && matches!(event.logical_key, Key::Named(NamedKey::F10))
                        {
                            announce_key_chord(
                                &event,
                                keyboard_modifiers,
                                InputWindow::Main,
                                &state,
                            );
                            dispatch(UiEvent::OpenContextMenu, &mut state, elwt);
                            return;
                        }
                        let navigation_event = match &event.logical_key {
                            Key::Named(NamedKey::Home) => Some(UiEvent::Home),
                            Key::Named(NamedKey::End) => Some(UiEvent::End),
                            Key::Named(NamedKey::PageUp) => Some(UiEvent::PageUp),
                            Key::Named(NamedKey::PageDown) => Some(UiEvent::PageDown),
                            _ => None,
                        };
                        if let Some(navigation_event) = navigation_event {
                            dispatch(navigation_event, &mut state, elwt);
                            return;
                        }
                        if state.focused_workspace_tab() && is_space_key(&event.logical_key) {
                            dispatch(UiEvent::Activate, &mut state, elwt);
                            return;
                        }
                        if state.side_panel_open() && is_space_key(&event.logical_key) {
                            dispatch(
                                UiEvent::KeyInput {
                                    text: " ".to_string(),
                                },
                                &mut state,
                                elwt,
                            );
                            return;
                        }
                        if !state.captures_modal_input()
                            && !state.is_editing_text()
                            && is_space_key(&event.logical_key)
                        {
                            dispatch_key_action(
                                UiAction::TogglePlayPause,
                                &event,
                                keyboard_modifiers,
                                InputWindow::Main,
                                &mut state,
                                elwt,
                            );
                            state.request_redraw();
                            return;
                        }
                        if !state.is_editing_text()
                            && (state.has_keyboard_focus() || state.side_panel_open())
                            && matches!(event.logical_key, Key::Named(NamedKey::Enter))
                        {
                            dispatch(UiEvent::Activate, &mut state, elwt);
                            return;
                        }
                        if !state.is_editing_text()
                            && (state.has_keyboard_focus() || state.side_panel_open())
                        {
                            let focused_navigation = match &event.logical_key {
                                Key::Named(NamedKey::ArrowLeft) => Some(UiEvent::CursorLeft),
                                Key::Named(NamedKey::ArrowRight) => Some(UiEvent::CursorRight),
                                Key::Named(NamedKey::ArrowUp) => Some(UiEvent::CursorUp),
                                Key::Named(NamedKey::ArrowDown) => Some(UiEvent::CursorDown),
                                Key::Named(NamedKey::Delete) => Some(UiEvent::Delete),
                                _ => None,
                            };
                            if let Some(focused_navigation) = focused_navigation {
                                dispatch(focused_navigation, &mut state, elwt);
                                return;
                            }
                        }

                        if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c.eq_ignore_ascii_case("k")) {
                            dispatch_key_action(
                                UiAction::SplitDialogue,
                                &event,
                                keyboard_modifiers,
                                InputWindow::Main,
                                &mut state,
                                elwt,
                            );
                            state.request_redraw();
                        } else if state.is_editing_text() {
                            if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c == "a") {
                                // Ctrl+A â€” select all text
                                dispatch(UiEvent::SelectAll, &mut state, elwt);
                                CommandDispatcher::announce_shortcut(&UiAction::SelectAll, &state);
                            } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c.eq_ignore_ascii_case("c")) {
                                dispatch(UiEvent::Copy, &mut state, elwt);
                                announce_named_text_command(
                                    crate::application::command::TextCommand::Copy,
                                    &state,
                                );
                            } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c.eq_ignore_ascii_case("x")) {
                                dispatch(UiEvent::Cut, &mut state, elwt);
                                announce_named_text_command(
                                    crate::application::command::TextCommand::Cut,
                                    &state,
                                );
                            } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c.eq_ignore_ascii_case("z")) {
                                dispatch(UiEvent::UndoTextEdit, &mut state, elwt);
                                announce_named_text_command(
                                    crate::application::command::TextCommand::Undo,
                                    &state,
                                );
                            } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c.eq_ignore_ascii_case("v")) {
                                // Ctrl+V â€” paste from clipboard
                                if let Some(text) = platform::clipboard_paste() {
                                    dispatch(UiEvent::KeyInput { text }, &mut state, elwt);
                                }
                                announce_named_text_command(
                                    crate::application::command::TextCommand::Paste,
                                    &state,
                                );
                            } else if matches!(event.logical_key, Key::Named(NamedKey::ArrowLeft)) {
                                if shift_held {
                                    announce_key_chord(
                                        &event,
                                        keyboard_modifiers,
                                        InputWindow::Main,
                                        &state,
                                    );
                                    dispatch(UiEvent::ShiftCursorLeft, &mut state, elwt);
                                } else {
                                    dispatch(UiEvent::CursorLeft, &mut state, elwt);
                                }
                            } else if matches!(event.logical_key, Key::Named(NamedKey::ArrowRight)) {
                                if shift_held {
                                    announce_key_chord(
                                        &event,
                                        keyboard_modifiers,
                                        InputWindow::Main,
                                        &state,
                                    );
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
                            dispatch_key_action(
                                UiAction::QuickSave,
                                &event,
                                keyboard_modifiers,
                                InputWindow::Main,
                                &mut state,
                                elwt,
                            );
                            state.request_redraw();
                        } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c.eq_ignore_ascii_case("a")) {
                            dispatch(UiEvent::SelectAll, &mut state, elwt);
                            CommandDispatcher::announce_shortcut(&UiAction::SelectAll, &state);
                        } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c.eq_ignore_ascii_case("c")) {
                            dispatch_key_action_with_explicit_announcement(
                                UiAction::CopySelectedLine,
                                &event,
                                keyboard_modifiers,
                                InputWindow::Main,
                                &mut state,
                                elwt,
                            );
                            state.request_redraw();
                        } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c.eq_ignore_ascii_case("x")) {
                            dispatch_key_action_with_explicit_announcement(
                                UiAction::CutSelectedLine,
                                &event,
                                keyboard_modifiers,
                                InputWindow::Main,
                                &mut state,
                                elwt,
                            );
                            state.request_redraw();
                        } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c.eq_ignore_ascii_case("v")) {
                            dispatch_key_action_with_explicit_announcement(
                                UiAction::PasteLine,
                                &event,
                                keyboard_modifiers,
                                InputWindow::Main,
                                &mut state,
                                elwt,
                            );
                            state.request_redraw();
                        } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c == "z") {
                            if event.repeat || !event.state.is_pressed() { /* skip */ } else {
                                dispatch_key_action_with_explicit_announcement(
                                    UiAction::Undo,
                                    &event,
                                    keyboard_modifiers,
                                    InputWindow::Main,
                                    &mut state,
                                    elwt,
                                );
                                state.request_redraw();
                            }
                        } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c == "n") {
                            dispatch_key_action(
                                UiAction::NewProject,
                                &event,
                                keyboard_modifiers,
                                InputWindow::Main,
                                &mut state,
                                elwt,
                            );
                            state.request_redraw();
                        } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c == "Z") {
                            // CTRL+SHIFT+Z = redo (capital Z)
                            dispatch_key_action_with_explicit_announcement(
                                UiAction::Redo,
                                &event,
                                keyboard_modifiers,
                                InputWindow::Main,
                                &mut state,
                                elwt,
                            );
                            state.request_redraw();
                        } else if matches!(event.logical_key, Key::Named(NamedKey::Delete)) {
                            announce_key_action(
                                &UiAction::DeleteSelected,
                                &event,
                                keyboard_modifiers,
                                InputWindow::Main,
                                &state,
                            );
                            dispatch(UiEvent::Delete, &mut state, elwt);
                        } else if is_space_key(&event.logical_key) {
                            dispatch_key_action(
                                UiAction::TogglePlayPause,
                                &event,
                                keyboard_modifiers,
                                InputWindow::Main,
                                &mut state,
                                elwt,
                            );
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
                        x: cursor_pos.0, y: cursor_pos.1, delta: scroll_delta, fast: shift_held, ctrl: ctrl_held,
                    }, &mut state, elwt);
                }
                WindowEvent::CursorMoved { position, .. } => {
                    cursor_pos = state.window_to_ui_position(position.x as f32, position.y as f32);
                    let now = Instant::now();
                    let pointer_interval = state.display_refresh_interval();
                    if last_pointer_dispatch
                        .is_none_or(|last| now.saturating_duration_since(last) >= pointer_interval)
                    {
                        dispatch(UiEvent::MouseMove {
                            x: cursor_pos.0, y: cursor_pos.1,
                        }, &mut state, elwt);
                        last_pointer_dispatch = Some(now);
                        last_dispatched_cursor_pos = cursor_pos;
                    }

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
                    let panel_resize_cursor = state.hovering_panel_resize_handle()
                        || state.dragging_panel_resize_handle();

                    let next_cursor_icon = if panel_resize_cursor {
                        winit::window::CursorIcon::EwResize
                    } else if resize_cursor {
                        winit::window::CursorIcon::NsResize
                    } else if is_text_cursor {
                        winit::window::CursorIcon::Text
                    } else {
                        winit::window::CursorIcon::Default
                    };
                    if next_cursor_icon != cursor_icon {
                        window.set_cursor_icon(next_cursor_icon);
                        cursor_icon = next_cursor_icon;
                    }
                }
                WindowEvent::Touch(touch) => {
                    // Windows tablets can expose the pen as a touch stream
                    // instead of a mouse stream. Feed that stream through the
                    // same pointer events used by the drawing tool.
                    cursor_pos = state.window_to_ui_position(
                        touch.location.x as f32,
                        touch.location.y as f32,
                    );
                    match touch.phase {
                        TouchPhase::Started => dispatch(
                            UiEvent::MousePress {
                                x: cursor_pos.0,
                                y: cursor_pos.1,
                            },
                            &mut state,
                            elwt,
                        ),
                        TouchPhase::Moved => dispatch(
                            UiEvent::MouseMove {
                                x: cursor_pos.0,
                                y: cursor_pos.1,
                            },
                            &mut state,
                            elwt,
                        ),
                        TouchPhase::Ended | TouchPhase::Cancelled => dispatch(
                            UiEvent::MouseRelease {
                                x: cursor_pos.0,
                                y: cursor_pos.1,
                            },
                            &mut state,
                            elwt,
                        ),
                    }
                    state.request_redraw();
                }
                WindowEvent::MouseInput {
                    state: ref button_state,
                    button: MouseButton::Left,
                    ..
                } => {
                    if cursor_pos != last_dispatched_cursor_pos {
                        dispatch(UiEvent::MouseMove {
                            x: cursor_pos.0,
                            y: cursor_pos.1,
                        }, &mut state, elwt);
                        last_pointer_dispatch = Some(Instant::now());
                        last_dispatched_cursor_pos = cursor_pos;
                    }
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
                WindowEvent::MouseInput {
                    state: ref button_state,
                    button: MouseButton::Right,
                    ..
                } => {
                    if matches!(button_state, ElementState::Pressed) {
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
                    if state.ui_shell.ui.has_active_progress() {
                        return;
                    }
                    let ext = path.extension()
                        .and_then(|e| e.to_str())
                        .map(|e| e.to_lowercase())
                        .unwrap_or_default();
                    if ["mp4", "mov", "avi", "mkv", "webm"].contains(&ext.as_str()) {
                        if state.load_video(&path) {
                            state.project_session.dirty = true;
                            state.request_redraw();
                        }
                    } else if [crate::project_archive::PROJECT_EXTENSION, "json"]
                        .contains(&ext.as_str())
                    {
                        state.start_br_import(path);
                    } else if ["srt", "ass", "detx"].contains(&ext.as_str()) {
                        super::file_picker::import_subtitle_from_path(&mut state, path);
                        state.request_redraw();
                    }
                }
                _ => {}
                }
            }
            Event::AboutToWait => {
                let changed = state.tick_background();
                #[cfg(target_os = "windows")]
                while let Ok(event) = accessibility_receiver.try_recv() {
                    windows_accessibility.announce(event);
                }
                if let Some(transition) = state.take_transition_after_save_ready() {
                    match transition {
                        crate::application::job_service::SaveContinuation::NewProject => {
                            new_project_reset_and_pick_video(&mut state, elwt)
                        }
                        crate::application::job_service::SaveContinuation::CloseProject => {
                            close_project_reset(&mut state)
                        }
                        crate::application::job_service::SaveContinuation::ExitApplication => {
                            elwt.exit()
                        }
                        crate::application::job_service::SaveContinuation::None => {}
                    }
                }
                if changed || state.needs_redraw_now() {
                    state.request_redraw();
                    if state.has_secondary_display() {
                        state.request_secondary_redraw();
                    }
                }

                // WaitUntil on Windows is not reliably granular enough for a
                // 4.166 ms bande-rythmo tick. During rythmo playback keep the
                // loop hot; State still gates actual redraws to exactly 240 Hz.
                if state.is_video_playing()
                    && state.active_workspace() == WorkspaceId::Rythmo
                {
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
