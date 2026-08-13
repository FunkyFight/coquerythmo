//! MP4 rendering for the ordered Comic Dubs playback.

use crate::comic_dubs::{Bubble, ComicDubsProject, Page, Point};
use crate::project::ExportConfiguration;
use image::{imageops, Rgba, RgbaImage};
use std::fmt::Write as _;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug)]
struct FrameStep {
    page_index: usize,
    visible_bubbles: usize,
    duration_ms: u64,
    audio: Option<(PathBuf, u64)>,
}

pub fn export_mp4(
    project: &ComicDubsProject,
    output: &Path,
    configuration: &ExportConfiguration,
    progress: Arc<AtomicU32>,
    cancel: Arc<AtomicBool>,
) -> Result<(), String> {
    let first_page = project
        .pages()
        .first()
        .ok_or_else(|| "Aucune page Comic Dubs à exporter".to_string())?;
    let fps = configuration.fps.clamp(1.0, 480.0);
    let (width, height) = crate::configured_export::resolve_video_dimensions(
        configuration,
        first_page.width,
        first_page.height,
    );
    let steps = playback_steps(project, fps);
    if steps.is_empty() {
        return Err("Aucune bulle Comic Dubs à exporter".into());
    }
    if let Some(parent) = output.parent().filter(|path| !path.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("Impossible de créer le dossier d'export : {error}"))?;
    }

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    let temp_dir =
        crate::media_binary::installation_temp_dir().join(format!("comic-dubs-export-{stamp}"));
    std::fs::create_dir_all(&temp_dir)
        .map_err(|error| format!("Dossier temporaire Comic Dubs : {error}"))?;
    let result = export_in_temp(
        project, output, fps, width, height, &steps, &temp_dir, &progress, &cancel,
    );
    let _ = std::fs::remove_dir_all(&temp_dir);
    result
}

#[allow(clippy::too_many_arguments)]
fn export_in_temp(
    project: &ComicDubsProject,
    output: &Path,
    fps: f64,
    width: u32,
    height: u32,
    steps: &[FrameStep],
    temp_dir: &Path,
    progress: &AtomicU32,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let mut concat = String::from("ffconcat version 1.0\n");
    let mut previous_page = usize::MAX;
    let mut page_canvas = RgbaImage::new(width, height);
    let mut page_rect = (0, 0, 0, 0);
    // ponytail: one PNG per bubble keeps the FFmpeg handoff simple; stream frames if
    // very large comics make temporary disk usage measurable.
    let mut frame_paths = Vec::with_capacity(steps.len());
    for (index, step) in steps.iter().enumerate() {
        check_cancel(cancel)?;
        if step.page_index != previous_page {
            let page = &project.pages()[step.page_index];
            (page_canvas, page_rect) = render_page(page, width, height)?;
            previous_page = step.page_index;
        }
        let page = &project.pages()[step.page_index];
        let mut canvas = page_canvas.clone();
        for (bubble_index, bubble) in page.bubbles.iter().enumerate() {
            let (show_background, _) = crate::comic_dubs::bubble_playback_state(
                bubble,
                bubble_index,
                step.visible_bubbles,
            );
            if show_background {
                render_bubble_background(&mut canvas, bubble, page_rect);
            }
        }
        for (bubble_index, bubble) in page.bubbles.iter().enumerate() {
            let (_, show_text) = crate::comic_dubs::bubble_playback_state(
                bubble,
                bubble_index,
                step.visible_bubbles,
            );
            if show_text {
                render_bubble_text(&mut canvas, bubble, page_rect, project.font_family());
            }
        }
        let frame_path = temp_dir.join(format!("frame-{index:06}.png"));
        canvas
            .save(&frame_path)
            .map_err(|error| format!("Image temporaire Comic Dubs : {error}"))?;
        let escaped = concat_path(&frame_path);
        writeln!(concat, "file '{escaped}'").unwrap();
        writeln!(concat, "duration {:.6}", step.duration_ms as f64 / 1_000.0).unwrap();
        frame_paths.push(frame_path);
        progress.store(
            (0.05 + 0.45 * (index + 1) as f32 / steps.len() as f32).to_bits(),
            Ordering::Relaxed,
        );
    }
    let last = concat_path(frame_paths.last().unwrap());
    writeln!(concat, "file '{last}'").unwrap();
    let concat_path = temp_dir.join("frames.ffconcat");
    std::fs::write(&concat_path, concat)
        .map_err(|error| format!("Timeline temporaire Comic Dubs : {error}"))?;

    let total_ms = steps.iter().map(|step| step.duration_ms).sum::<u64>();
    let cues = steps
        .iter()
        .filter_map(|step| step.audio.as_ref())
        .collect::<Vec<_>>();
    let mut command = crate::media_binary::command("ffmpeg");
    command
        .args(["-v", "warning", "-y", "-f", "concat", "-safe", "0", "-i"])
        .arg(&concat_path);
    for (path, _) in &cues {
        command.arg("-i").arg(path);
    }
    if !cues.is_empty() {
        let mut filter = String::new();
        for (index, (_, start_ms)) in cues.iter().enumerate() {
            write!(
                filter,
                "[{}:a]adelay={}:all=1[a{}];",
                index + 1,
                start_ms,
                index
            )
            .unwrap();
        }
        for index in 0..cues.len() {
            write!(filter, "[a{index}]").unwrap();
        }
        write!(
            filter,
            "amix=inputs={}:normalize=0:duration=longest[a]",
            cues.len()
        )
        .unwrap();
        command.args(["-filter_complex", &filter, "-map", "0:v:0", "-map", "[a]"]);
    } else {
        command.args(["-map", "0:v:0", "-an"]);
    }
    command.args([
        "-r",
        &fps.to_string(),
        "-t",
        &format!("{:.6}", total_ms as f64 / 1_000.0),
        "-fps_mode",
        "cfr",
        "-c:v",
        "libx264",
        "-preset",
        "veryfast",
        "-crf",
        "18",
        "-pix_fmt",
        "yuv420p",
    ]);
    if !cues.is_empty() {
        command.args(["-c:a", "aac", "-b:a", "192k"]);
    }
    command
        .args(["-movflags", "+faststart"])
        .arg(output)
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    progress.store(0.55_f32.to_bits(), Ordering::Relaxed);
    let mut child = command
        .spawn()
        .map_err(|error| format!("Démarrage de FFmpeg : {error}"))?;
    let stderr = child.stderr.take().map(|mut stderr| {
        std::thread::spawn(move || {
            let mut text = String::new();
            let _ = stderr.read_to_string(&mut text);
            text
        })
    });
    let status = loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = std::fs::remove_file(output);
            return Err(crate::video_export::EXPORT_CANCELLED_MESSAGE.into());
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("Attente de FFmpeg : {error}"))?
        {
            break status;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let stderr = stderr
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();
    if !status.success() {
        let _ = std::fs::remove_file(output);
        return Err(if stderr.trim().is_empty() {
            "Échec de l'export MP4 Comic Dubs".into()
        } else {
            format!("Échec de l'export MP4 Comic Dubs : {}", stderr.trim())
        });
    }
    progress.store(1.0_f32.to_bits(), Ordering::Relaxed);
    Ok(())
}

fn playback_steps(project: &ComicDubsProject, fps: f64) -> Vec<FrameStep> {
    let playable_pages = project
        .pages()
        .iter()
        .enumerate()
        .filter(|(_, page)| !page.bubbles.is_empty())
        .collect::<Vec<_>>();
    let minimum_ms = (1_000.0 / fps.max(1.0)).ceil() as u64;
    let mut elapsed_ms = 0_u64;
    let mut steps = Vec::new();
    for (playable_index, (page_index, page)) in playable_pages.iter().enumerate() {
        for (bubble_index, bubble) in page.bubbles.iter().enumerate() {
            let audio = bubble
                .audio_id
                .and_then(|id| project.audio(id))
                .map(|audio| (audio.playback_path.clone(), elapsed_ms));
            let audio_ms = bubble
                .audio_id
                .and_then(|id| project.audio(id))
                .map_or(0, |audio| audio.duration_ms());
            let gap_ms = if bubble_index + 1 < page.bubbles.len() {
                project.bubble_gap_ms()
            } else if playable_index + 1 < playable_pages.len() {
                project.page_gap_ms()
            } else {
                project.page_gap_ms().max(1_000)
            };
            let duration_ms = audio_ms.saturating_add(gap_ms).max(minimum_ms);
            steps.push(FrameStep {
                page_index: *page_index,
                visible_bubbles: bubble_index + 1,
                duration_ms,
                audio,
            });
            elapsed_ms = elapsed_ms.saturating_add(duration_ms);
        }
    }
    steps
}

fn render_page(
    page: &Page,
    width: u32,
    height: u32,
) -> Result<(RgbaImage, (u32, u32, u32, u32)), String> {
    let source = image::open(&page.image_path)
        .map_err(|error| format!("Page {} illisible : {error}", page.file_name))?
        .to_rgba8();
    let scale = (width as f64 / source.width() as f64).min(height as f64 / source.height() as f64);
    let page_width = (source.width() as f64 * scale).round().max(1.0) as u32;
    let page_height = (source.height() as f64 * scale).round().max(1.0) as u32;
    let x = (width - page_width) / 2;
    let y = (height - page_height) / 2;
    let resized = imageops::resize(
        &source,
        page_width,
        page_height,
        imageops::FilterType::Lanczos3,
    );
    let mut canvas = RgbaImage::from_pixel(width, height, Rgba([0, 0, 0, 255]));
    imageops::overlay(&mut canvas, &resized, i64::from(x), i64::from(y));
    Ok((canvas, (x, y, page_width, page_height)))
}

fn render_bubble_background(canvas: &mut RgbaImage, bubble: &Bubble, rect: (u32, u32, u32, u32)) {
    let points = bubble
        .points
        .iter()
        .map(|point| {
            (
                rect.0 as f32 + point.x * rect.2 as f32,
                rect.1 as f32 + point.y * rect.3 as f32,
            )
        })
        .collect::<Vec<_>>();
    fill_polygon(canvas, &points, Rgba(bubble.color));
    for (a, b) in points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
    {
        draw_line(canvas, *a, *b, Rgba([225, 225, 235, 255]));
    }
}

fn render_bubble_text(
    canvas: &mut RgbaImage,
    bubble: &Bubble,
    rect: (u32, u32, u32, u32),
    font_family: Option<&str>,
) {
    let bounds = polygon_text_bounds(&bubble.points);
    let text_rect = (
        rect.0 as f32 + bounds.0 * rect.2 as f32,
        rect.1 as f32 + bounds.1 * rect.3 as f32,
        (bounds.2 - bounds.0) * rect.2 as f32,
        (bounds.3 - bounds.1) * rect.3 as f32,
    );
    let preferred = bubble.font_size * canvas.height() as f32 / 1080.0;
    let (lines, font_size) = fit_text(&bubble.text, text_rect.2, text_rect.3, preferred);
    let line_height = font_size * 1.18;
    let mut y = text_rect.1 + (text_rect.3 - line_height * lines.len() as f32) * 0.5;
    let color = if luminance(bubble.color) > 0.55 {
        [24, 24, 30]
    } else {
        [244, 244, 248]
    };
    for line in lines {
        let measured = crate::vector_text::measure_text_width_with_family_standalone(
            &line,
            font_size,
            font_family,
        )
        .unwrap_or(text_rect.2)
        .ceil()
        .clamp(1.0, text_rect.2.max(1.0)) as u32;
        if let Some(pixmap) = crate::vector_text::render_text_natural_with_family_standalone(
            &line,
            font_size,
            measured,
            line_height.ceil().max(1.0) as u32,
            font_family,
        ) {
            let x = text_rect.0 + (text_rect.2 - pixmap.width as f32) * 0.5;
            blend_text(
                canvas,
                &pixmap.pixels,
                pixmap.width,
                pixmap.height,
                x,
                y,
                color,
            );
        }
        y += line_height;
    }
}

fn fill_polygon(image: &mut RgbaImage, points: &[(f32, f32)], color: Rgba<u8>) {
    let min_y = points
        .iter()
        .map(|point| point.1)
        .fold(f32::MAX, f32::min)
        .floor()
        .max(0.0) as u32;
    let max_y = points
        .iter()
        .map(|point| point.1)
        .fold(f32::MIN, f32::max)
        .ceil()
        .min(image.height() as f32) as u32;
    for y in min_y..max_y {
        let sample_y = y as f32 + 0.5;
        let mut intersections = Vec::new();
        for (a, b) in points
            .iter()
            .zip(points.iter().cycle().skip(1))
            .take(points.len())
        {
            if (a.1 > sample_y) != (b.1 > sample_y) {
                intersections.push(a.0 + (sample_y - a.1) * (b.0 - a.0) / (b.1 - a.1));
            }
        }
        intersections.sort_by(f32::total_cmp);
        for span in intersections.chunks_exact(2) {
            let start = span[0].floor().max(0.0) as u32;
            let end = span[1].ceil().min(image.width() as f32) as u32;
            for x in start..end {
                image.put_pixel(x, y, color);
            }
        }
    }
}

fn draw_line(image: &mut RgbaImage, a: (f32, f32), b: (f32, f32), color: Rgba<u8>) {
    let steps = (b.0 - a.0).abs().max((b.1 - a.1).abs()).ceil() as u32;
    for step in 0..=steps {
        let ratio = step as f32 / steps.max(1) as f32;
        let x = (a.0 + (b.0 - a.0) * ratio).round() as i32;
        let y = (a.1 + (b.1 - a.1) * ratio).round() as i32;
        if x >= 0 && y >= 0 && x < image.width() as i32 && y < image.height() as i32 {
            image.put_pixel(x as u32, y as u32, color);
        }
    }
}

fn blend_text(
    image: &mut RgbaImage,
    mask: &[u8],
    width: u32,
    height: u32,
    x: f32,
    y: f32,
    color: [u8; 3],
) {
    let origin_x = x.round() as i32;
    let origin_y = y.round() as i32;
    for source_y in 0..height {
        for source_x in 0..width {
            let target_x = origin_x + source_x as i32;
            let target_y = origin_y + source_y as i32;
            if target_x < 0
                || target_y < 0
                || target_x >= image.width() as i32
                || target_y >= image.height() as i32
            {
                continue;
            }
            let alpha = mask[((source_y * width + source_x) * 4 + 3) as usize] as u16;
            if alpha == 0 {
                continue;
            }
            let pixel = image.get_pixel_mut(target_x as u32, target_y as u32);
            for channel in 0..3 {
                pixel[channel] = ((u16::from(color[channel]) * alpha
                    + u16::from(pixel[channel]) * (255 - alpha))
                    / 255) as u8;
            }
            pixel[3] = 255;
        }
    }
}

fn polygon_text_bounds(points: &[Point]) -> (f32, f32, f32, f32) {
    // ponytail: the inset bounding box matches ordinary bubbles; reuse the editor's
    // concave-cell search if highly concave export bubbles become a real case.
    let min_x = points.iter().map(|point| point.x).fold(1.0, f32::min);
    let max_x = points.iter().map(|point| point.x).fold(0.0, f32::max);
    let min_y = points.iter().map(|point| point.y).fold(1.0, f32::min);
    let max_y = points.iter().map(|point| point.y).fold(0.0, f32::max);
    let inset_x = (max_x - min_x) * 0.08;
    let inset_y = (max_y - min_y) * 0.08;
    (
        min_x + inset_x,
        min_y + inset_y,
        max_x - inset_x,
        max_y - inset_y,
    )
}

fn fit_text(text: &str, width: f32, height: f32, preferred: f32) -> (Vec<String>, f32) {
    let maximum = preferred.clamp(6.0, 144.0).floor() as u32;
    for font in (6..=maximum).rev().map(|size| size as f32) {
        let max_chars = (width / (font * 0.56)).floor().max(1.0) as usize;
        let lines = wrap_text(text, max_chars);
        if lines.len() as f32 * font * 1.18 <= height {
            return (lines, font);
        }
    }
    (
        wrap_text(text, (width / 3.36).floor().max(1.0) as usize),
        6.0,
    )
}

fn wrap_text(text: &str, max_chars: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if !current.is_empty() && current.chars().count() + 1 + word.chars().count() > max_chars {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn luminance(color: [u8; 4]) -> f32 {
    color[0] as f32 / 255.0 * 0.2126
        + color[1] as f32 / 255.0 * 0.7152
        + color[2] as f32 / 255.0 * 0.0722
}

fn concat_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .replace('\'', "'\\''")
}

fn check_cancel(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        Err(crate::video_export::EXPORT_CANCELLED_MESSAGE.into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recording::{RecordedAudio, WaveformData};

    #[test]
    fn playback_plan_delays_audio_and_keeps_the_final_frame() {
        let mut project = ComicDubsProject::default();
        project.set_gaps(250, 900);
        let page = project.add_page("page.png".into(), "page.png".into(), 100, 100);
        let audio = project.add_audio(
            "voice.flac".into(),
            "voice.flac".into(),
            RecordedAudio {
                file_name: "voice.flac".into(),
                sample_rate: 1_000,
                channels: 1,
                sample_count: 500,
                checksum: String::new(),
                waveform: WaveformData::default(),
            },
        );
        let points = vec![
            Point { x: 0.1, y: 0.1 },
            Point { x: 0.9, y: 0.1 },
            Point { x: 0.5, y: 0.9 },
        ];
        let first = project.add_bubble(page, points.clone()).unwrap();
        project.assign_audio(first, Some(audio));
        project.add_bubble(page, points).unwrap();

        let steps = playback_steps(&project, 25.0);
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].duration_ms, 750);
        assert_eq!(steps[0].audio.as_ref().unwrap().1, 0);
        assert_eq!(steps[1].duration_ms, 1_000);
    }

    #[test]
    fn exports_a_playable_mp4() {
        if !crate::media_binary::can_run("ffmpeg") {
            return;
        }
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("coquerythmo-comic-test-{stamp}"));
        std::fs::create_dir_all(&directory).unwrap();
        let page_path = directory.join("page.png");
        RgbaImage::from_pixel(64, 64, Rgba([20, 30, 40, 255]))
            .save(&page_path)
            .unwrap();
        let output = directory.join("comic.mp4");
        let audio_path = directory.join("voice.wav");
        let samples = [0_i16; 800];
        let data_len = (samples.len() * 2) as u32;
        let mut wav = Vec::with_capacity(44 + data_len as usize);
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data_len).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16_u32.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&8_000_u32.to_le_bytes());
        wav.extend_from_slice(&16_000_u32.to_le_bytes());
        wav.extend_from_slice(&2_u16.to_le_bytes());
        wav.extend_from_slice(&16_u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_len.to_le_bytes());
        wav.extend(samples.iter().flat_map(|sample| sample.to_le_bytes()));
        std::fs::write(&audio_path, wav).unwrap();
        let mut project = ComicDubsProject::default();
        let page = project.add_page("page.png".into(), page_path, 64, 64);
        let bubble = project
            .add_bubble(
                page,
                vec![
                    Point { x: 0.1, y: 0.1 },
                    Point { x: 0.9, y: 0.1 },
                    Point { x: 0.5, y: 0.9 },
                ],
            )
            .unwrap();
        let audio = project.add_audio(
            "voice.wav".into(),
            audio_path,
            RecordedAudio {
                file_name: "voice.wav".into(),
                sample_rate: 8_000,
                channels: 1,
                sample_count: samples.len() as u64,
                checksum: String::new(),
                waveform: WaveformData::default(),
            },
        );
        project.assign_audio(bubble, Some(audio));
        export_mp4(
            &project,
            &output,
            &ExportConfiguration::default(),
            Arc::new(AtomicU32::new(0)),
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        assert!(std::fs::metadata(&output).unwrap().len() > 100);
        let probe = crate::media_binary::command("ffprobe")
            .args([
                "-v",
                "error",
                "-select_streams",
                "a:0",
                "-show_entries",
                "stream=codec_type",
                "-of",
                "default=nw=1:nk=1",
            ])
            .arg(&output)
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&probe.stdout).trim(), "audio");
        let _ = std::fs::remove_dir_all(directory);
    }
}
