//! OS-specific windows, icon and clipboard adapters.

use winit::window::WindowBuilder;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "macos")]
use winit::platform::macos::WindowBuilderExtMacOS;

#[cfg(target_os = "windows")]
pub(crate) fn cursor_position(window: &winit::window::Window) -> Option<(f32, f32)> {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let mut point = POINT { x: 0, y: 0 };
    let RawWindowHandle::Win32(handle) = window.window_handle().ok()?.as_raw() else {
        return None;
    };
    if unsafe { GetCursorPos(&mut point) } == 0
        || unsafe { ScreenToClient(handle.hwnd.get() as _, &mut point) } == 0
    {
        return None;
    }
    Some((point.x as f32, point.y as f32))
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn cursor_position(_window: &winit::window::Window) -> Option<(f32, f32)> {
    None
}

pub(crate) fn app_icon() -> Option<winit::window::Icon> {
    parse_ico_to_winit_icon(include_bytes!("icons/app.ico"))
}

pub(crate) fn window_builder() -> WindowBuilder {
    let builder = WindowBuilder::new();
    configure_platform_window(builder)
}

/// Register the portable project format for the current Windows user.
///
/// The association is deliberately stored under HKCU, so opening a project
/// never requires elevation. Re-running the app also refreshes the executable
/// path when the portable folder has been moved.
#[cfg(target_os = "windows")]
pub(crate) fn register_project_file_association() {
    let Ok(executable) = std::env::current_exe() else {
        return;
    };
    let command = format!("\"{}\" \"%1\"", executable.display());
    let icon = format!("\"{}\",0", executable.display());
    let executable_name = executable
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("coquerythmo.exe");
    let application_key = format!(r"HKCU\Software\Classes\Applications\{executable_name}");
    let entries = [
        (
            r"HKCU\Software\Classes\.coquerythmo".to_string(),
            "Coquerythmo.Project".to_string(),
        ),
        (
            r"HKCU\Software\Classes\Coquerythmo.Project".to_string(),
            "Projet Coquerythmo".to_string(),
        ),
        (
            r"HKCU\Software\Classes\Coquerythmo.Project\DefaultIcon".to_string(),
            icon,
        ),
        (
            r"HKCU\Software\Classes\Coquerythmo.Project\shell\open".to_string(),
            "Ouvrir avec Coquerythmo".to_string(),
        ),
        (
            r"HKCU\Software\Classes\Coquerythmo.Project\shell\open\command".to_string(),
            command.clone(),
        ),
        (
            r"HKCU\Software\Classes\Coquerythmo.Project\shell\open_with_coquerythmo".to_string(),
            "Ouvrir avec Coquerythmo".to_string(),
        ),
        (
            r"HKCU\Software\Classes\Coquerythmo.Project\shell\open_with_coquerythmo\command"
                .to_string(),
            command.clone(),
        ),
        (application_key.clone(), "Coquerythmo".to_string()),
        (format!(r"{application_key}\shell\open\command"), command),
    ];
    for (key, value) in entries {
        if !add_registry_value(&key, None, &value) {
            return;
        }
    }

    // These named values make Coquerythmo appear in Windows' "Open with"
    // chooser even when another application is currently the default.
    if !add_registry_value(
        r"HKCU\Software\Classes\.coquerythmo\OpenWithProgids",
        Some("Coquerythmo.Project"),
        "",
    ) || !add_registry_value(
        &format!(r"{application_key}\SupportedTypes"),
        Some(".coquerythmo"),
        "",
    ) {
        return;
    }

    const SHCNE_ASSOCCHANGED: i32 = 0x0800_0000;
    const SHCNF_IDLIST: u32 = 0;
    unsafe {
        windows_sys::Win32::UI::Shell::SHChangeNotify(
            SHCNE_ASSOCCHANGED,
            SHCNF_IDLIST,
            std::ptr::null(),
            std::ptr::null(),
        );
    }
}

#[cfg(target_os = "windows")]
fn add_registry_value(key: &str, name: Option<&str>, value: &str) -> bool {
    let mut command = std::process::Command::new("reg.exe");
    command.creation_flags(0x0800_0000).args(["ADD", key]);
    if let Some(name) = name {
        command.args(["/v", name]);
    } else {
        command.arg("/ve");
    }
    match command.args(["/t", "REG_SZ", "/d", value, "/f"]).status() {
        Ok(status) if status.success() => true,
        Ok(status) => {
            log::warn!("Could not register project file association ({status})");
            false
        }
        Err(error) => {
            log::warn!("Could not register project file association: {error}");
            false
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn register_project_file_association() {}

/// Register the `coquerythmo://` URL scheme so browsers and other apps can
/// launch/join sessions from a link. Stored under HKCU, no elevation needed.
#[cfg(target_os = "windows")]
pub(crate) fn register_url_protocol() {
    use crate::protocol::PROTOCOL_SCHEME;

    let Ok(executable) = std::env::current_exe() else {
        return;
    };
    let command = format!("\"{}\" \"%1\"", executable.display());
    let icon = format!("\"{}\",0", executable.display());
    let base = format!(r"HKCU\Software\Classes\{PROTOCOL_SCHEME}");
    let entries = [
        (
            base.clone(),
            "URL:Coquerythmo quick session link".to_string(),
        ),
        (format!(r"{base}\DefaultIcon"), icon),
        (format!(r"{base}\shell\open\command"), command),
    ];
    for (key, value) in entries {
        if !add_registry_value(&key, None, &value) {
            return;
        }
    }
    // The named (empty) REG_SZ `URL Protocol` value marks the key as a URI
    // scheme handler for Windows ShellExecute.
    if !add_registry_value(&base, Some("URL Protocol"), "") {
        return;
    }

    const SHCNE_ASSOCCHANGED: i32 = 0x0800_0000;
    const SHCNF_IDLIST: u32 = 0;
    unsafe {
        windows_sys::Win32::UI::Shell::SHChangeNotify(
            SHCNE_ASSOCCHANGED,
            SHCNF_IDLIST,
            std::ptr::null(),
            std::ptr::null(),
        );
    }
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn register_url_protocol() {}

#[cfg(target_os = "macos")]
fn configure_platform_window(builder: WindowBuilder) -> WindowBuilder {
    builder.with_accepts_first_mouse(true)
}

#[cfg(not(target_os = "macos"))]
fn configure_platform_window(builder: WindowBuilder) -> WindowBuilder {
    builder
}

/// Parse an ICO file and return a winit Icon (RGBA pixels).
/// Picks the largest image entry, renders it via resvg's tiny-skia PNG decoder if PNG,
/// or falls back to raw BMP parsing.
pub(crate) fn parse_ico_to_winit_icon(ico_data: &[u8]) -> Option<winit::window::Icon> {
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
pub(crate) fn clipboard_set(text: &str) {
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
pub(crate) fn clipboard_set(_text: &str) {}

#[cfg(target_os = "windows")]
pub(crate) fn clipboard_paste() -> Option<String> {
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
pub(crate) fn clipboard_paste() -> Option<String> {
    None
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) fn show_untested_platform_warning() {
    let platform = if cfg!(target_os = "macos") {
        "macOS"
    } else {
        "Linux"
    };
    let message = format!(
        "Cette version {platform} de Coquerythmo n'a pas pu être testée correctement, car je n'ai pas d'appareil Linux ni macOS pour la tester.\n\nElle peut fonctionner comme prévu, mais elle peut aussi ne pas fonctionner ou comporter des bugs spécifiques à cette plateforme."
    );

    let _ = rfd::MessageDialog::new()
        .set_title("Version non testée")
        .set_description(&message)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(crate) fn show_untested_platform_warning() {}
