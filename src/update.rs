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

pub fn promote_pending_updater_from_args() {
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        if arg != std::ffi::OsStr::new(PROMOTE_UPDATER_ARG) {
            continue;
        }

        let Some(pending) = args.next() else {
            log::warn!("{} passed without path", PROMOTE_UPDATER_ARG);
            continue;
        };

        let pending = std::path::PathBuf::from(pending);
        match promote_pending_updater(&pending) {
            Ok(()) => log::info!("Promoted updater from {}", pending.display()),
            Err(e) => log::warn!(
                "Failed to promote updater from {}: {}",
                pending.display(),
                e
            ),
        }
    }
}

fn promote_pending_updater(pending: &std::path::Path) -> Result<(), String> {
    if !pending.is_file() {
        return Err(format!("pending updater not found: {}", pending.display()));
    }

    let exe_dir = std::env::current_exe()
        .map_err(|e| format!("Cannot find exe path: {e}"))?
        .parent()
        .ok_or("Cannot get exe directory")?
        .to_path_buf();
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

    match Command::new(&updater).arg("--tag").arg(&latest_tag).spawn() {
        Ok(_) => {
            log::info!("Updater launched, exiting for update");
            true
        }
        Err(e) => {
            log::error!("Failed to launch updater: {}", e);
            false
        }
    }
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
