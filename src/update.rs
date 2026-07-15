#[cfg(not(target_os = "windows"))]
use std::process::Command;

const GITHUB_RELEASES_API: &str =
    "https://api.github.com/repos/funkyfight/coquerythmo-releases/releases";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const PROMOTE_UPDATER_ARG: &str = "--coquerythmo-promote-updater";
const PROMOTE_UPDATER_ATTEMPTS: usize = 50;
const PROMOTE_UPDATER_RETRY_MS: u64 = 100;

#[derive(Debug, Clone)]
pub struct ReleaseInfo {
    pub tag_name: String,
    pub body: String,
}

pub fn current_version() -> &'static str {
    CURRENT_VERSION
}

pub fn current_tag() -> String {
    format!("v{}", CURRENT_VERSION)
}

fn updater_executable_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "updater.exe"
    } else {
        "updater"
    }
}

pub fn promote_pending_updater_at_startup() {
    let exe_dir = match std::env::current_exe()
        .map_err(|e| format!("Cannot find exe path: {e}"))
        .and_then(|path| {
            path.parent()
                .map(std::path::Path::to_path_buf)
                .ok_or_else(|| "Cannot get exe directory".to_string())
        }) {
        Ok(exe_dir) => exe_dir,
        Err(e) => {
            log::warn!("Failed to locate a pending updater: {e}");
            return;
        }
    };

    // The pending file is the durable hand-off between updater.exe and the app.
    // Do not rely solely on process arguments: launchers and shortcuts are allowed
    // to discard them, while the file remains next to coquerythmo.exe.
    let requested_pending = pending_updater_from_args();
    let pending = pending_updater_to_promote(&exe_dir, requested_pending);

    let Some(pending) = pending else {
        return;
    };

    match promote_pending_updater(&pending, &exe_dir) {
        Ok(()) => log::info!("Promoted updater from {}", pending.display()),
        Err(e) => log::warn!(
            "Failed to promote updater from {}: {}",
            pending.display(),
            e
        ),
    }
}

fn pending_updater_from_args() -> Option<std::path::PathBuf> {
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        if arg != std::ffi::OsStr::new(PROMOTE_UPDATER_ARG) {
            continue;
        }

        let Some(pending) = args.next() else {
            log::warn!("{} passed without path", PROMOTE_UPDATER_ARG);
            return None;
        };

        return Some(std::path::PathBuf::from(pending));
    }

    None
}

fn pending_updater_path(exe_dir: &std::path::Path) -> std::path::PathBuf {
    exe_dir.join(format!("{}.pending", updater_executable_name()))
}

fn pending_updater_to_promote(
    exe_dir: &std::path::Path,
    requested_pending: Option<std::path::PathBuf>,
) -> Option<std::path::PathBuf> {
    let expected_pending = pending_updater_path(exe_dir);
    if expected_pending.is_file() {
        Some(expected_pending)
    } else {
        requested_pending
    }
}

fn promote_pending_updater(
    pending: &std::path::Path,
    exe_dir: &std::path::Path,
) -> Result<(), String> {
    if !pending.is_file() {
        return Err(format!("pending updater not found: {}", pending.display()));
    }

    let target = exe_dir.join(updater_executable_name());
    let backup = exe_dir.join(format!("{}.old", updater_executable_name()));

    let mut last_error = None;
    for attempt in 0..PROMOTE_UPDATER_ATTEMPTS {
        match try_promote_pending_updater(pending, &target, &backup) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last_error = Some(e);
                if attempt + 1 < PROMOTE_UPDATER_ATTEMPTS {
                    std::thread::sleep(std::time::Duration::from_millis(PROMOTE_UPDATER_RETRY_MS));
                }
            }
        }
    }

    Err(last_error.unwrap_or_else(|| "unknown updater promotion error".to_string()))
}

fn try_promote_pending_updater(
    pending: &std::path::Path,
    target: &std::path::Path,
    backup: &std::path::Path,
) -> Result<(), String> {
    let backup_created = if target.exists() {
        if backup.exists() {
            std::fs::remove_file(backup)
                .map_err(|e| format!("remove previous updater backup: {e}"))?;
        }
        std::fs::rename(target, backup).map_err(|e| format!("move current updater aside: {e}"))?;
        true
    } else {
        backup.exists()
    };

    match std::fs::rename(pending, target) {
        Ok(()) => {
            if backup_created {
                let _ = std::fs::remove_file(backup);
            }
            Ok(())
        }
        Err(e) => {
            if backup_created && !target.exists() && backup.exists() {
                let _ = std::fs::rename(backup, target);
            }
            Err(format!("install pending updater: {e}"))
        }
    }
}

/// Check for updates in the background. Returns true if the app should exit (updater launched).
pub fn check() -> bool {
    let current_tag = current_tag();
    log::info!("Update check: current version {}", current_tag);

    let latest_release = match fetch_latest_release() {
        Ok(release) => release,
        Err(e) => {
            log::warn!("Update check failed: {}", e);
            return false;
        }
    };
    let latest_tag = latest_release.tag_name;

    log::info!("Update check: latest version {}", latest_tag);

    if latest_tag == current_tag {
        log::info!("Already up to date");
        return false;
    }

    // Ask user via native dialog
    let message = format!(
        "Une nouvelle version est disponible : {}\nVersion actuelle : {}\n\nInstaller la mise à jour ?",
        latest_tag, current_tag
    );

    let accepted = rfd::MessageDialog::new()
        .set_title("Mise à jour disponible")
        .set_description(&message)
        .set_buttons(rfd::MessageButtons::YesNo)
        .show()
        == rfd::MessageDialogResult::Yes;

    if !accepted {
        log::info!("Update declined by user");
        return false;
    }

    // Launch the platform-specific updater next to our executable.
    let exe_dir = match std::env::current_exe() {
        Ok(p) => p
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .to_path_buf(),
        Err(e) => {
            log::error!("Cannot find exe path: {}", e);
            return false;
        }
    };

    let updater_name = updater_executable_name();
    let updater = exe_dir.join(updater_name);
    if !updater.exists() {
        log::error!("{} not found at {}", updater_name, updater.display());
        return false;
    }

    match launch_updater(&updater, &latest_tag) {
        Ok(()) => {
            log::info!("Updater launched, exiting for update");
            true
        }
        Err(e) => {
            log::error!("Failed to launch updater: {}", e);
            false
        }
    }
}

#[cfg(target_os = "windows")]
fn launch_updater(updater: &std::path::Path, latest_tag: &str) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let operation: Vec<u16> = "runas\0".encode_utf16().collect();
    let executable: Vec<u16> = updater
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let parameters: Vec<u16> = format!("--tag {}", quote_windows_argument(latest_tag))
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    // ShellExecuteW with the `runas` verb displays the Windows UAC prompt.
    // The elevated updater then relaunches coquerythmo with the inherited token.
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            operation.as_ptr(),
            executable.as_ptr(),
            parameters.as_ptr(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };

    if result as isize > 32 {
        Ok(())
    } else {
        Err(format!(
            "Windows refused to launch the updater as administrator (ShellExecuteW code {})",
            result as isize
        ))
    }
}

#[cfg(target_os = "windows")]
fn quote_windows_argument(argument: &str) -> String {
    if !argument.is_empty()
        && !argument
            .chars()
            .any(|character| character.is_whitespace() || character == '"')
    {
        return argument.to_string();
    }

    let mut quoted = String::from("\"");
    let mut backslashes = 0;
    for character in argument.chars() {
        if character == '\\' {
            backslashes += 1;
            continue;
        }

        if character == '"' {
            quoted.extend(std::iter::repeat_n('\\', backslashes * 2 + 1));
            quoted.push('"');
        } else {
            quoted.extend(std::iter::repeat_n('\\', backslashes));
            quoted.push(character);
        }
        backslashes = 0;
    }

    quoted.extend(std::iter::repeat_n('\\', backslashes * 2));
    quoted.push('"');
    quoted
}

#[cfg(not(target_os = "windows"))]
fn launch_updater(updater: &std::path::Path, latest_tag: &str) -> Result<(), String> {
    Command::new(updater)
        .arg("--tag")
        .arg(latest_tag)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub fn fetch_latest_release() -> Result<ReleaseInfo, String> {
    fetch_release(&format!("{GITHUB_RELEASES_API}/latest"))
}

pub fn fetch_release_by_tag(tag: &str) -> Result<ReleaseInfo, String> {
    fetch_release(&format!("{GITHUB_RELEASES_API}/tags/{tag}"))
}

fn fetch_release(url: &str) -> Result<ReleaseInfo, String> {
    let response: serde_json::Value = ureq::get(url)
        .header("User-Agent", "coquerythmo-updater")
        .call()
        .map_err(|e| format!("HTTP request failed: {e}"))?
        .body_mut()
        .read_json()
        .map_err(|e| format!("JSON parse failed: {e}"))?;

    let tag_name = response["tag_name"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "No tag_name in response".to_string())?;
    let body = response["body"].as_str().unwrap_or_default().to_string();

    Ok(ReleaseInfo { tag_name, body })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(test_name: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after the Unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "coquerythmo-{test_name}-{}-{unique}",
            std::process::id()
        ))
    }

    #[test]
    fn promotes_the_default_pending_updater_and_removes_the_backup() {
        let exe_dir = temp_dir("promote-updater");
        fs::create_dir_all(&exe_dir).unwrap();

        let target = exe_dir.join(updater_executable_name());
        let pending = pending_updater_path(&exe_dir);
        let backup = exe_dir.join(format!("{}.old", updater_executable_name()));
        fs::write(&target, b"old updater").unwrap();
        fs::write(&pending, b"new updater").unwrap();

        let discovered = pending_updater_to_promote(&exe_dir, None).unwrap();
        promote_pending_updater(&discovered, &exe_dir).unwrap();

        assert_eq!(fs::read(&target).unwrap(), b"new updater");
        assert!(!pending.exists());
        assert!(!backup.exists());

        fs::remove_dir_all(exe_dir).unwrap();
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn quotes_windows_updater_arguments() {
        assert_eq!(quote_windows_argument("v3.4.0"), "v3.4.0");
        assert_eq!(quote_windows_argument("v 3.4.0"), "\"v 3.4.0\"");
        assert_eq!(quote_windows_argument("v\"3"), "\"v\\\"3\"");
        assert_eq!(quote_windows_argument("v 3\\"), "\"v 3\\\\\"");
    }
}
