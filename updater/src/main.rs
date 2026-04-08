use std::env;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;

const GITHUB_API: &str = "https://api.github.com/repos/funkyfight/coquerythmo-releases/releases/tags";

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
    println!("Updating to {}...", tag);

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

    // Find the -windows-portable.zip asset
    let assets = release["assets"]
        .as_array()
        .ok_or("No assets in release")?;

    let zip_asset = assets
        .iter()
        .find(|a| {
            a["name"]
                .as_str()
                .map(|n| n.ends_with("-windows-portable.zip"))
                .unwrap_or(false)
        })
        .ok_or("No -windows-portable.zip asset found")?;

    let download_url = zip_asset["browser_download_url"]
        .as_str()
        .ok_or("No download URL for asset")?;

    let file_size = zip_asset["size"].as_u64().unwrap_or(0);
    println!("Downloading {} ({:.1} MB)...", download_url, file_size as f64 / 1_048_576.0);

    // Download to temp file
    let temp_zip = exe_dir.join("_update.zip");
    download_file(download_url, &temp_zip)?;
    println!("Download complete");

    // Extract zip
    println!("Extracting files...");
    extract_zip(&temp_zip, &exe_dir)?;

    // Clean up temp file
    let _ = fs::remove_file(&temp_zip);

    // Relaunch coquerythmo
    let coquerythmo = exe_dir.join("coquerythmo.exe");
    println!("Launching {}...", coquerythmo.display());

    Command::new(&coquerythmo)
        .spawn()
        .map_err(|e| format!("Failed to launch coquerythmo: {e}"))?;

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
    Err("Usage: updater.exe --tag <tag>".to_string())
}

fn download_file(url: &str, dest: &Path) -> Result<(), String> {
    let response = ureq::get(url)
        .header("User-Agent", "coquerythmo-updater")
        .call()
        .map_err(|e| format!("Download failed: {e}"))?;

    let mut body = response.into_body().into_reader();
    let mut file = fs::File::create(dest)
        .map_err(|e| format!("Cannot create temp file: {e}"))?;

    let mut buf = [0u8; 65536];
    loop {
        let n = body.read(&mut buf).map_err(|e| format!("Read error: {e}"))?;
        if n == 0 { break; }
        file.write_all(&buf[..n]).map_err(|e| format!("Write error: {e}"))?;
    }

    Ok(())
}

fn extract_zip(zip_path: &Path, dest_dir: &PathBuf) -> Result<(), String> {
    let file = fs::File::open(zip_path)
        .map_err(|e| format!("Cannot open zip: {e}"))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| format!("Invalid zip: {e}"))?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)
            .map_err(|e| format!("Zip entry error: {e}"))?;

        let name = entry.name().to_string();

        // Skip directories and paths with .. (security)
        if name.ends_with('/') || name.contains("..") {
            continue;
        }

        // Don't overwrite ourselves while running
        if name == "updater.exe" {
            continue;
        }

        let out_path = dest_dir.join(&name);

        // Create parent directories if needed
        if let Some(parent) = out_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let mut out_file = fs::File::create(&out_path)
            .map_err(|e| format!("Cannot create {}: {e}", name))?;

        io::copy(&mut entry, &mut out_file)
            .map_err(|e| format!("Cannot write {}: {e}", name))?;

        println!("  {}", name);
    }

    Ok(())
}
