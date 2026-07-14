//! OS-specific windows, icon and clipboard adapters.

use winit::window::WindowBuilder;

#[cfg(target_os = "macos")]
use winit::platform::macos::WindowBuilderExtMacOS;

pub(crate) fn app_icon() -> Option<winit::window::Icon> {
    parse_ico_to_winit_icon(include_bytes!("icons/app.ico"))
}

pub(crate) fn window_builder() -> WindowBuilder {
    let builder = WindowBuilder::new();
    configure_platform_window(builder)
}

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
