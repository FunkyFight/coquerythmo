use std::fs;
use std::path::{Path, PathBuf};

fn replace_exact(source: &mut String, old: &str, new: &str, expected: usize, file: &str) {
    let count = source.matches(old).count();
    assert_eq!(
        count, expected,
        "unexpected occurrence count for renderer patch in {file}: expected {expected}, got {count}\npattern: {old}"
    );
    *source = source.replace(old, new);
}

fn make_include_safe(source: &mut String) {
    if source.starts_with("//! ") {
        source.replace_range(..3, "//");
    }
    *source = source.replace("\n//!", "\n//");
    *source = source.replace(
        "#![allow(clippy::too_many_arguments)]",
        "// clippy::too_many_arguments is allowed by the parent module",
    );
}

fn generate_renderer(template: &Path, output: &Path) {
    let file = template.display().to_string();
    let mut source = fs::read_to_string(template)
        .unwrap_or_else(|error| panic!("failed to read {file}: {error}"));

    replace_exact(
        &mut source,
        "        let center_x = w / 2.0;\n        let offset_frames = crate::config::reading_bar_offset_seconds() * source_fps;",
        "        let center_x = w / 2.0;\n        let offset_frames = crate::config::reading_bar_offset_seconds() * source_fps;\n        let reading_bar_x = center_x - offset_frames as f32 * ppf;",
        1,
        &file,
    );
    replace_exact(
        &mut source,
        "let playhead_x = center_x - playhead_w / 2.0 - offset_frames as f32 * ppf;",
        "let playhead_x = reading_bar_x - playhead_w / 2.0;",
        1,
        &file,
    );
    replace_exact(
        &mut source,
        "let karaoke_left = center_x - karaoke_width / 2.0;",
        "let karaoke_left = reading_bar_x - karaoke_width / 2.0;",
        1,
        &file,
    );
    replace_exact(
        &mut source,
        "let karaoke_right = center_x + karaoke_width / 2.0;",
        "let karaoke_right = reading_bar_x + karaoke_width / 2.0;",
        1,
        &file,
    );
    replace_exact(
        &mut source,
        "(center_x - width / 2.0, width)",
        "(reading_bar_x - width / 2.0, width)",
        2,
        &file,
    );

    make_include_safe(&mut source);
    fs::write(output, source)
        .unwrap_or_else(|error| panic!("failed to write {}: {error}", output.display()));
}

fn generate_geometry(template: &Path, output: &Path) {
    let file = template.display().to_string();
    let mut source = fs::read_to_string(template)
        .unwrap_or_else(|error| panic!("failed to read {file}: {error}"));

    replace_exact(
        &mut source,
        "    let center_x = zone.x + zone.width / 2.0;\n    let offset_frames = reading_bar_offset_seconds * fps;",
        "    let center_x = zone.x + zone.width / 2.0;\n    let offset_frames = reading_bar_offset_seconds * fps;\n    let reading_bar_x = center_x - offset_frames as f32 * ppf();",
        1,
        &file,
    );
    replace_exact(
        &mut source,
        "return (center_x - width / 2.0, width);",
        "return (reading_bar_x - width / 2.0, width);",
        1,
        &file,
    );

    make_include_safe(&mut source);
    fs::write(output, source)
        .unwrap_or_else(|error| panic!("failed to write {}: {error}", output.display()));
}

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "windows" {
        let mut res = winres::WindowsResource::new();
        res.set_icon("src/icons/app.ico");
        res.compile().expect("Failed to compile Windows resources");
    }

    println!("cargo:rerun-if-changed=src/rythmo_cpu_renderer.template.rs");
    println!("cargo:rerun-if-changed=src/rythmo_gpu_renderer.template.rs");
    println!("cargo:rerun-if-changed=src/workspaces/rythmo/geometry.template.rs");

    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR is not set"));
    generate_renderer(
        Path::new("src/rythmo_cpu_renderer.template.rs"),
        &out_dir.join("rythmo_cpu_renderer.rs"),
    );
    generate_renderer(
        Path::new("src/rythmo_gpu_renderer.template.rs"),
        &out_dir.join("rythmo_gpu_renderer.rs"),
    );
    generate_geometry(
        Path::new("src/workspaces/rythmo/geometry.template.rs"),
        &out_dir.join("rythmo_geometry.rs"),
    );
}
