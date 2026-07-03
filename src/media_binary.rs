use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub(crate) fn command(binary: &str) -> Command {
    if let Some(path) = path(binary) {
        log::info!("Using {binary}: {}", path.display());
        Command::new(path)
    } else {
        log::warn!(
            "{binary} not found. current_exe={:?}, current_dir={:?}, COQUERYTHMO_FFMPEG_DIR={:?}",
            std::env::current_exe(),
            std::env::current_dir(),
            std::env::var_os("COQUERYTHMO_FFMPEG_DIR")
        );
        Command::new(binary)
    }
}

pub(crate) fn can_run(binary: &str) -> bool {
    command(binary)
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub(crate) fn path(binary: &str) -> Option<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(dir) = std::env::var_os("COQUERYTHMO_FFMPEG_DIR") {
        push_binary_candidates(&mut candidates, Path::new(&dir), binary);
    }

    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(exe_dir) = current_exe.parent() {
            push_binary_candidates(&mut candidates, exe_dir, binary);
            push_binary_candidates(&mut candidates, &exe_dir.join("bin"), binary);
        }

        if let Some(app_bundle) = current_exe.ancestors().find(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("app"))
        }) {
            let resources_dir = app_bundle.join("Contents").join("Resources");
            push_binary_candidates(&mut candidates, &resources_dir, binary);
            push_binary_candidates(&mut candidates, &resources_dir.join("bin"), binary);

            if let Some(app_parent) = app_bundle.parent() {
                push_binary_candidates(&mut candidates, app_parent, binary);
                push_binary_candidates(&mut candidates, &app_parent.join("bin"), binary);
            }
        }
    }

    if let Ok(current_dir) = std::env::current_dir() {
        push_binary_candidates(&mut candidates, &current_dir, binary);
        push_binary_candidates(&mut candidates, &current_dir.join("bin"), binary);
    }

    #[cfg(target_os = "macos")]
    candidates.extend(
        ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"]
            .iter()
            .map(|dir| Path::new(dir).join(binary)),
    );

    candidates.into_iter().find(|path| path.is_file())
}

fn push_binary_candidates(candidates: &mut Vec<PathBuf>, dir: &Path, binary: &str) {
    candidates.push(dir.join(binary));

    #[cfg(windows)]
    if Path::new(binary).extension().is_none() {
        candidates.push(dir.join(format!("{binary}.exe")));
    }
}
