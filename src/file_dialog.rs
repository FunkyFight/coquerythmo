use std::path::{Path, PathBuf};

#[derive(Clone, Copy)]
pub struct FileFilter<'a> {
    pub name: &'a str,
    pub extensions: &'a [&'a str],
}

pub fn open_file(
    title: &str,
    filters: &[FileFilter<'_>],
    initial_dir: Option<&Path>,
) -> Option<PathBuf> {
    platform::open_file(title, filters, initial_dir)
}

pub fn save_file(
    title: &str,
    filters: &[FileFilter<'_>],
    initial_dir: Option<&Path>,
    default_extension: &str,
) -> Option<PathBuf> {
    platform::save_file(title, filters, initial_dir, default_extension)
        .map(|path| with_default_extension(path, default_extension))
}

fn with_default_extension(mut path: PathBuf, extension: &str) -> PathBuf {
    if path.extension().is_none() {
        path.set_extension(extension);
    }
    path
}

#[cfg(target_os = "windows")]
mod platform {
    use super::FileFilter;
    use std::ffi::c_void;
    use std::ffi::OsString;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::path::{Path, PathBuf};

    const BUFFER_LEN: usize = 32_768;
    const OFN_OVERWRITEPROMPT: u32 = 0x0000_0002;
    const OFN_HIDEREADONLY: u32 = 0x0000_0004;
    const OFN_NOCHANGEDIR: u32 = 0x0000_0008;
    const OFN_PATHMUSTEXIST: u32 = 0x0000_0800;
    const OFN_FILEMUSTEXIST: u32 = 0x0000_1000;

    #[allow(non_snake_case)]
    #[repr(C)]
    struct OpenFileNameW {
        lStructSize: u32,
        hwndOwner: *mut c_void,
        hInstance: *mut c_void,
        lpstrFilter: *const u16,
        lpstrCustomFilter: *mut u16,
        nMaxCustFilter: u32,
        nFilterIndex: u32,
        lpstrFile: *mut u16,
        nMaxFile: u32,
        lpstrFileTitle: *mut u16,
        nMaxFileTitle: u32,
        lpstrInitialDir: *const u16,
        lpstrTitle: *const u16,
        Flags: u32,
        nFileOffset: u16,
        nFileExtension: u16,
        lpstrDefExt: *const u16,
        lCustData: isize,
        lpfnHook: *mut c_void,
        lpTemplateName: *const u16,
        pvReserved: *mut c_void,
        dwReserved: u32,
        FlagsEx: u32,
    }

    #[link(name = "Comdlg32")]
    extern "system" {
        fn GetOpenFileNameW(open_file_name: *mut OpenFileNameW) -> i32;
        fn GetSaveFileNameW(open_file_name: *mut OpenFileNameW) -> i32;
        fn CommDlgExtendedError() -> u32;
    }

    pub fn open_file(
        title: &str,
        filters: &[FileFilter<'_>],
        initial_dir: Option<&Path>,
    ) -> Option<PathBuf> {
        run_dialog(DialogKind::Open, title, filters, initial_dir, None)
    }

    pub fn save_file(
        title: &str,
        filters: &[FileFilter<'_>],
        initial_dir: Option<&Path>,
        default_extension: &str,
    ) -> Option<PathBuf> {
        run_dialog(
            DialogKind::Save,
            title,
            filters,
            initial_dir,
            Some(default_extension),
        )
    }

    enum DialogKind {
        Open,
        Save,
    }

    fn run_dialog(
        kind: DialogKind,
        title: &str,
        filters: &[FileFilter<'_>],
        initial_dir: Option<&Path>,
        default_extension: Option<&str>,
    ) -> Option<PathBuf> {
        let title_wide = wide_null(title);
        let filters_wide = build_filter_list(filters);
        let initial_dir_wide = initial_dir.map(path_to_wide_null);
        let default_extension_wide = default_extension.map(wide_null);
        let mut file_buffer = vec![0_u16; BUFFER_LEN];

        let mut ofn = OpenFileNameW {
            lStructSize: std::mem::size_of::<OpenFileNameW>() as u32,
            hwndOwner: std::ptr::null_mut(),
            hInstance: std::ptr::null_mut(),
            lpstrFilter: filters_wide.as_ptr(),
            lpstrCustomFilter: std::ptr::null_mut(),
            nMaxCustFilter: 0,
            nFilterIndex: 1,
            lpstrFile: file_buffer.as_mut_ptr(),
            nMaxFile: file_buffer.len() as u32,
            lpstrFileTitle: std::ptr::null_mut(),
            nMaxFileTitle: 0,
            lpstrInitialDir: initial_dir_wide
                .as_ref()
                .map_or(std::ptr::null(), |path| path.as_ptr()),
            lpstrTitle: title_wide.as_ptr(),
            Flags: dialog_flags(&kind),
            nFileOffset: 0,
            nFileExtension: 0,
            lpstrDefExt: default_extension_wide
                .as_ref()
                .map_or(std::ptr::null(), |extension| extension.as_ptr()),
            lCustData: 0,
            lpfnHook: std::ptr::null_mut(),
            lpTemplateName: std::ptr::null(),
            pvReserved: std::ptr::null_mut(),
            dwReserved: 0,
            FlagsEx: 0,
        };

        let ok = unsafe {
            match kind {
                DialogKind::Open => GetOpenFileNameW(&mut ofn),
                DialogKind::Save => GetSaveFileNameW(&mut ofn),
            }
        };

        if ok == 0 {
            let error = unsafe { CommDlgExtendedError() };
            if error != 0 {
                log::warn!("Windows file dialog failed: 0x{error:04x}");
            }
            return None;
        }

        path_from_buffer(&file_buffer)
    }

    fn dialog_flags(kind: &DialogKind) -> u32 {
        let common = OFN_HIDEREADONLY | OFN_NOCHANGEDIR | OFN_PATHMUSTEXIST;
        match kind {
            DialogKind::Open => common | OFN_FILEMUSTEXIST,
            DialogKind::Save => common | OFN_OVERWRITEPROMPT,
        }
    }

    fn build_filter_list(filters: &[FileFilter<'_>]) -> Vec<u16> {
        let mut out = Vec::new();

        if filters.is_empty() {
            push_wide_entry(&mut out, "All Files");
            push_wide_entry(&mut out, "*.*");
        } else {
            for filter in filters {
                push_wide_entry(&mut out, filter.name);
                push_wide_entry(&mut out, &pattern_for_extensions(filter.extensions));
            }
        }

        out.push(0);
        out
    }

    fn pattern_for_extensions(extensions: &[&str]) -> String {
        if extensions.is_empty() {
            return "*.*".into();
        }

        extensions
            .iter()
            .map(|extension| {
                if *extension == "*" {
                    "*.*".into()
                } else {
                    format!("*.{extension}")
                }
            })
            .collect::<Vec<_>>()
            .join(";")
    }

    fn push_wide_entry(out: &mut Vec<u16>, value: &str) {
        out.extend(value.encode_utf16());
        out.push(0);
    }

    fn wide_null(value: &str) -> Vec<u16> {
        let mut out: Vec<_> = value.encode_utf16().collect();
        out.push(0);
        out
    }

    fn path_to_wide_null(path: &Path) -> Vec<u16> {
        let mut out: Vec<_> = path.as_os_str().encode_wide().collect();
        out.push(0);
        out
    }

    fn path_from_buffer(buffer: &[u16]) -> Option<PathBuf> {
        let len = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
        if len == 0 {
            return None;
        }
        Some(PathBuf::from(OsString::from_wide(&buffer[..len])))
    }
}

#[cfg(not(target_os = "windows"))]
mod platform {
    use super::FileFilter;
    use std::path::{Path, PathBuf};

    pub fn open_file(
        title: &str,
        filters: &[FileFilter<'_>],
        initial_dir: Option<&Path>,
    ) -> Option<PathBuf> {
        let mut dialog = rfd::FileDialog::new().set_title(title);
        for filter in filters {
            dialog = dialog.add_filter(filter.name, filter.extensions);
        }
        if let Some(initial_dir) = initial_dir {
            dialog = dialog.set_directory(initial_dir);
        }
        dialog.pick_file()
    }

    pub fn save_file(
        title: &str,
        filters: &[FileFilter<'_>],
        initial_dir: Option<&Path>,
        _default_extension: &str,
    ) -> Option<PathBuf> {
        let mut dialog = rfd::FileDialog::new().set_title(title);
        for filter in filters {
            dialog = dialog.add_filter(filter.name, filter.extensions);
        }
        if let Some(initial_dir) = initial_dir {
            dialog = dialog.set_directory(initial_dir);
        }
        dialog.save_file()
    }
}
