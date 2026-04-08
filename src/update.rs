use std::process::Command;

const GITHUB_API: &str = "https://api.github.com/repos/funkyfight/coquerythmo-releases/releases/latest";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Check for updates in the background. Returns true if the app should exit (updater launched).
pub fn check() -> bool {
    let current_tag = format!("v{}", CURRENT_VERSION);
    log::info!("Update check: current version {}", current_tag);

    let latest_tag = match fetch_latest_tag() {
        Ok(tag) => tag,
        Err(e) => {
            log::warn!("Update check failed: {}", e);
            return false;
        }
    };

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
        .show() == rfd::MessageDialogResult::Yes;

    if !accepted {
        log::info!("Update declined by user");
        return false;
    }

    // Launch updater.exe next to our executable
    let exe_dir = match std::env::current_exe() {
        Ok(p) => p.parent().unwrap_or(std::path::Path::new(".")).to_path_buf(),
        Err(e) => {
            log::error!("Cannot find exe path: {}", e);
            return false;
        }
    };

    let updater = exe_dir.join("updater.exe");
    if !updater.exists() {
        log::error!("updater.exe not found at {}", updater.display());
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

fn fetch_latest_tag() -> Result<String, String> {
    let response: serde_json::Value = ureq::get(GITHUB_API)
        .header("User-Agent", "coquerythmo-updater")
        .call()
        .map_err(|e| format!("HTTP request failed: {e}"))?
        .body_mut()
        .read_json()
        .map_err(|e| format!("JSON parse failed: {e}"))?;

    response["tag_name"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "No tag_name in response".to_string())
}
