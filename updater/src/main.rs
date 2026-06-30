use std::env;
use std::fs;
use std::io::{self, Read, Write};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;

const APP_NAME: &str = "coquerythmo";
const GITHUB_API: &str =
    "https://api.github.com/repos/funkyfight/coquerythmo-releases/releases/tags";

fn main() {
    if let Err(e) = run() {
        eprintln!("Update failed: {}", e);
        // Wait so user can read the error
        thread::sleep(Duration::from_secs(5));
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let tag = parse_tag_arg()?;
    println!("Updating to {} for {}...", tag, target_label());

    let exe_dir = env::current_exe()
        .map_err(|e| format!("Cannot find exe path: {e}"))?
        .parent()
        .ok_or("Cannot get exe directory")?
        .to_path_buf();

    // Wait for coquerythmo to exit
    println!("Waiting for coquerythmo to close...");
    thread::sleep(Duration::from_secs(2));

    // Fetch release info
    let url = format!("{}/{}", GITHUB_API, tag);
    println!("Fetching release info from {}", url);

    let release: serde_json::Value = ureq::get(&url)
        .header("User-Agent", "coquerythmo-updater")
        .call()
        .map_err(|e| format!("HTTP request failed: {e}"))?
        .body_mut()
        .read_json()
        .map_err(|e| format!("JSON parse failed: {e}"))?;

    let assets = release["assets"].as_array().ok_or("No assets in release")?;

    let zip_asset = find_release_asset(assets)?;
    let asset_name = zip_asset["name"].as_str().unwrap_or("release asset");

    let download_url = zip_asset["browser_download_url"]
        .as_str()
        .ok_or("No download URL for asset")?;

    let file_size = zip_asset["size"].as_u64().unwrap_or(0);
    println!(
        "Downloading {} from {} ({:.1} MB)...",
        asset_name,
        download_url,
        file_size as f64 / 1_048_576.0
    );

    // Download to temp file
    let temp_zip = update_archive_path(&tag);
    let _ = fs::remove_file(&temp_zip);
    download_file(download_url, &temp_zip)?;
    println!("Download complete");

    // Extract zip
    println!("Extracting files...");
    extract_zip(&temp_zip, &exe_dir)?;

    // Clean up temp file
    let _ = fs::remove_file(&temp_zip);

    launch_app(&exe_dir)?;

    println!("Update complete!");
    Ok(())
}

fn parse_tag_arg() -> Result<String, String> {
    let args: Vec<String> = env::args().collect();
    for i in 0..args.len() {
        if args[i] == "--tag" {
            if let Some(tag) = args.get(i + 1) {
                return Ok(tag.clone());
            }
        }
    }
    Err(format!("Usage: {} --tag <tag>", updater_executable_name()))
}

fn find_release_asset(assets: &[serde_json::Value]) -> Result<&serde_json::Value, String> {
    let suffixes = asset_name_suffixes();

    assets
        .iter()
        .find(|asset| {
            asset["name"]
                .as_str()
                .map(|name| {
                    let name = name.to_ascii_lowercase();
                    suffixes.iter().any(|suffix| name.ends_with(suffix))
                })
                .unwrap_or(false)
        })
        .ok_or_else(|| {
            format!(
                "No release zip asset found for {}; expected suffixes: {}",
                target_label(),
                suffixes.join(", ")
            )
        })
}

fn asset_name_suffixes() -> Vec<String> {
    let mut suffixes = Vec::new();
    let platforms = platform_aliases();
    let archs = arch_aliases();

    for platform in platforms {
        for arch in &archs {
            suffixes.push(format!("-{platform}-{arch}-portable.zip"));
            suffixes.push(format!("-{platform}-{arch}.zip"));
        }
    }

    if cfg!(target_os = "macos") {
        for platform in platforms {
            suffixes.push(format!("-{platform}-universal-portable.zip"));
            suffixes.push(format!("-{platform}-universal.zip"));
        }
    }

    for platform in platforms {
        suffixes.push(format!("-{platform}-portable.zip"));
        suffixes.push(format!("-{platform}.zip"));
    }

    suffixes
}

fn platform_aliases() -> &'static [&'static str] {
    match env::consts::OS {
        "windows" => &["windows", "win32", "win"],
        "macos" => &["macos", "darwin", "osx"],
        "linux" => &["linux"],
        _ => &[],
    }
}

fn arch_aliases() -> Vec<&'static str> {
    let mut aliases = vec![env::consts::ARCH];
    let extras: &[&str] = match env::consts::ARCH {
        "x86_64" => &["x64", "amd64"],
        "aarch64" => &["arm64"],
        "x86" => &["i686"],
        _ => &[],
    };
    aliases.extend_from_slice(extras);
    aliases
}

fn target_label() -> String {
    format!("{}-{}", env::consts::OS, env::consts::ARCH)
}

fn update_archive_path(tag: &str) -> PathBuf {
    env::temp_dir().join(format!(
        "coquerythmo-{}-{}-update.zip",
        sanitize_file_component(tag),
        target_label()
    ))
}

fn sanitize_file_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "update".to_string()
    } else {
        out
    }
}

fn application_executable_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "coquerythmo.exe"
    } else {
        APP_NAME
    }
}

fn updater_executable_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "updater.exe"
    } else {
        "updater"
    }
}

fn launch_app(exe_dir: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    if let Some(bundle) = macos_app_bundle_from_exe_dir(exe_dir) {
        println!("Launching {}...", bundle.display());
        return Command::new("open")
            .arg("-n")
            .arg(&bundle)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Failed to launch macOS app bundle: {e}"));
    }

    let coquerythmo = exe_dir.join(application_executable_name());
    println!("Launching {}...", coquerythmo.display());

    Command::new(&coquerythmo)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Failed to launch coquerythmo: {e}"))
}

#[cfg(target_os = "macos")]
fn macos_app_bundle_from_exe_dir(exe_dir: &Path) -> Option<PathBuf> {
    let contents_dir = exe_dir.parent()?;
    if contents_dir.file_name()?.to_string_lossy() != "Contents" {
        return None;
    }

    let bundle_dir = contents_dir.parent()?;
    if bundle_dir.extension()?.to_string_lossy() != "app" {
        return None;
    }

    Some(bundle_dir.to_path_buf())
}

fn download_file(url: &str, dest: &Path) -> Result<(), String> {
    let response = ureq::get(url)
        .header("User-Agent", "coquerythmo-updater")
        .call()
        .map_err(|e| format!("Download failed: {e}"))?;

    let mut body = response.into_body().into_reader();
    let mut file = fs::File::create(dest).map_err(|e| format!("Cannot create temp file: {e}"))?;

    let mut buf = [0u8; 65536];
    loop {
        let n = body
            .read(&mut buf)
            .map_err(|e| format!("Read error: {e}"))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])
            .map_err(|e| format!("Write error: {e}"))?;
    }

    Ok(())
}

fn extract_zip(zip_path: &Path, dest_dir: &Path) -> Result<(), String> {
    let file = fs::File::open(zip_path).map_err(|e| format!("Cannot open zip: {e}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("Invalid zip: {e}"))?;

    // Use \\?\ prefix on Windows to support long paths (>260 chars)
    let dest_str = dest_dir.to_string_lossy().to_string();
    let long_dest: PathBuf = if cfg!(target_os = "windows") && !dest_str.starts_with(r"\\?\") {
        PathBuf::from(format!("\\\\?\\{}", dest_str))
    } else {
        dest_dir.to_path_buf()
    };

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("Zip entry error: {e}"))?;

        let raw_name = entry.name().to_string();
        let Some(name) = entry.enclosed_name().map(|path| path.to_path_buf()) else {
            continue;
        };

        if entry.is_dir() {
            let _ = fs::create_dir_all(long_dest.join(&name));
            continue;
        }

        // Don't overwrite ourselves while running
        if is_running_updater_entry(&name) {
            continue;
        }

        let out_path = long_dest.join(&name);

        // Create parent directories — if a file blocks the path, nuke it
        if let Some(parent) = out_path.parent() {
            if let Err(_) = fs::create_dir_all(parent) {
                // Something blocked it — try removing any file in the way
                let mut p = parent.to_path_buf();
                while p != long_dest && p.exists() && !p.is_dir() {
                    let _ = fs::remove_file(&p);
                    p = match p.parent() {
                        Some(pp) => pp.to_path_buf(),
                        None => break,
                    };
                }
                let _ = fs::create_dir_all(parent);
            }
        }

        // Best-effort write — skip on failure
        match fs::File::create(&out_path) {
            Ok(mut out_file) => {
                if io::copy(&mut entry, &mut out_file).is_ok() {
                    #[cfg(unix)]
                    {
                        let mode = entry.unix_mode().unwrap_or_else(|| {
                            if is_executable_entry(&name) {
                                0o755
                            } else {
                                0o644
                            }
                        });
                        let _ = fs::set_permissions(
                            &out_path,
                            fs::Permissions::from_mode(mode & 0o777),
                        );
                    }
                    println!("  {}", name.display());
                }
            }
            Err(_) => {
                eprintln!("  [skip] {}", raw_name);
            }
        }
    }

    Ok(())
}

fn is_running_updater_entry(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| name == updater_executable_name())
        .unwrap_or(false)
}

#[cfg(unix)]
fn is_executable_entry(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| name == application_executable_name() || name == updater_executable_name())
        .unwrap_or(false)
}
