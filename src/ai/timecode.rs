//! Timecodes and durations as people say them to the assistant.

/// Frames for a position written as `HH:MM:SS:FF`, `MM:SS`, `1:30.5`,
/// `90s`, `1m30`, `1 minute 30`, `2h`, `frame 120` / `image 120`, or a bare
/// number of seconds.
pub fn parse_frames(text: &str, fps: f64) -> Option<i64> {
    let fps = if fps > 0.0 { fps } else { 25.0 };
    let text = text.trim().to_lowercase().replace(',', ".");
    if text.is_empty() {
        return None;
    }
    if text.contains(':') {
        return parse_colon_timecode(&text, fps);
    }
    let (seconds, frames) = parse_units(&text)?;
    Some((seconds * fps).round() as i64 + frames)
}

fn parse_colon_timecode(text: &str, fps: f64) -> Option<i64> {
    let parts: Vec<&str> = text.split(':').map(str::trim).collect();
    let numbers: Option<Vec<f64>> = parts.iter().map(|part| part.parse::<f64>().ok()).collect();
    let numbers = numbers?;
    let (seconds, frames) = match numbers.as_slice() {
        [m, s] => (m * 60.0 + s, 0.0),
        [h, m, s] => (h * 3600.0 + m * 60.0 + s, 0.0),
        [h, m, s, f] => (h * 3600.0 + m * 60.0 + s, *f),
        _ => return None,
    };
    Some((seconds * fps).round() as i64 + frames.round() as i64)
}

/// `(seconds, frames)` of a text made of numbers and unit words.
fn parse_units(text: &str) -> Option<(f64, i64)> {
    let mut tokens: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_is_digit = false;
    for c in text.chars() {
        let is_digit = c.is_ascii_digit() || c == '.';
        if c.is_whitespace() || c == '-' {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            continue;
        }
        if !current.is_empty() && is_digit != current_is_digit {
            tokens.push(std::mem::take(&mut current));
        }
        current_is_digit = is_digit;
        current.push(c);
    }
    if !current.is_empty() {
        tokens.push(current);
    }

    let mut seconds = 0.0;
    let mut frames = 0i64;
    let mut pending: Option<f64> = None;
    let mut last_unit: Option<Unit> = None;
    let mut any = false;
    let mut frames_prefix = false;
    for token in &tokens {
        if let Ok(value) = token.parse::<f64>() {
            if frames_prefix {
                frames += value.round() as i64;
                frames_prefix = false;
                any = true;
                continue;
            }
            if let Some(previous) = pending.take() {
                // "1 minute 30": a bare number after a unit is the next smaller unit.
                match last_unit {
                    Some(Unit::Hours) => seconds += previous * 60.0,
                    Some(Unit::Minutes) => seconds += previous,
                    _ => seconds += previous,
                }
            }
            pending = Some(value);
            any = true;
            continue;
        }
        let Some(unit) = Unit::parse(token) else {
            return None;
        };
        match unit {
            Unit::Frames if pending.is_none() => {
                frames_prefix = true;
            }
            _ => {
                let value = pending.take().unwrap_or(0.0);
                match unit {
                    Unit::Hours => seconds += value * 3600.0,
                    Unit::Minutes => seconds += value * 60.0,
                    Unit::Seconds => seconds += value,
                    Unit::Frames => frames += value.round() as i64,
                }
                last_unit = Some(unit);
            }
        }
    }
    if let Some(value) = pending {
        match last_unit {
            Some(Unit::Hours) => seconds += value * 60.0,
            Some(Unit::Minutes) => seconds += value,
            _ => seconds += value,
        }
    }
    any.then_some((seconds, frames))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Unit {
    Hours,
    Minutes,
    Seconds,
    Frames,
}

impl Unit {
    fn parse(word: &str) -> Option<Self> {
        let word = super::retrieval::normalize(word);
        let word = word.trim();
        let stem = |prefixes: &[&str]| prefixes.iter().any(|prefix| word.starts_with(prefix));
        if word == "h" || stem(&["heure", "hour", "hora", "hr"]) {
            Some(Unit::Hours)
        } else if word == "m" || word == "mn" || stem(&["min"]) {
            Some(Unit::Minutes)
        } else if word == "s" || stem(&["sec", "seg"]) {
            Some(Unit::Seconds)
        } else if word == "f" || word == "fr" || stem(&["frame", "image", "img", "fotogram", "trame"]) {
            Some(Unit::Frames)
        } else {
            None
        }
    }
}

/// Seconds for a duration such as `2`, `2.5 s`, `1 min 30` or `0:45`.
pub fn parse_seconds(text: &str) -> Option<f64> {
    let text = text.trim().to_lowercase().replace(',', ".");
    if text.is_empty() {
        return None;
    }
    if text.contains(':') {
        return parse_colon_timecode(&text, 1.0).map(|frames| frames as f64);
    }
    parse_units(&text).map(|(seconds, frames)| seconds + frames as f64 / 25.0)
}

/// `HH:MM:SS:FF` for a frame.
pub fn format_timecode(frame: i64, fps: f64) -> String {
    let fps = if fps > 0.0 { fps } else { 25.0 };
    let frame = frame.max(0);
    let whole_fps = fps.round().max(1.0) as i64;
    let total_seconds = (frame as f64 / fps).floor() as i64;
    let frames = frame - (total_seconds as f64 * fps).round() as i64;
    let frames = frames.clamp(0, whole_fps - 1);
    format!(
        "{:02}:{:02}:{:02}:{:02}",
        total_seconds / 3600,
        (total_seconds / 60) % 60,
        total_seconds % 60,
        frames
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_spoken_positions() {
        assert_eq!(parse_frames("1:30", 25.0), Some(2250));
        assert_eq!(parse_frames("00:01:30:12", 25.0), Some(2262));
        assert_eq!(parse_frames("1:02:03", 25.0), Some((3723) * 25));
        assert_eq!(parse_frames("90s", 25.0), Some(2250));
        assert_eq!(parse_frames("1m30", 25.0), Some(2250));
        assert_eq!(parse_frames("1 minute 30", 25.0), Some(2250));
        assert_eq!(parse_frames("1 minute 30 secondes", 25.0), Some(2250));
        assert_eq!(parse_frames("2h", 25.0), Some(7200 * 25));
        assert_eq!(parse_frames("2 min", 25.0), Some(3000));
        assert_eq!(parse_frames("45", 25.0), Some(1125));
        assert_eq!(parse_frames("1.5s", 24.0), Some(36));
        assert_eq!(parse_frames("frame 120", 25.0), Some(120));
        assert_eq!(parse_frames("image 120", 25.0), Some(120));
        assert_eq!(parse_frames("120 images", 25.0), Some(120));
        assert_eq!(parse_frames("1h 2m 3s", 25.0), Some(3723 * 25));
        assert_eq!(parse_frames("", 25.0), None);
        assert_eq!(parse_frames("plus tard", 25.0), None);
    }

    #[test]
    fn parses_durations() {
        assert_eq!(parse_seconds("2"), Some(2.0));
        assert_eq!(parse_seconds("2,5 s"), Some(2.5));
        assert_eq!(parse_seconds("1 min 30"), Some(90.0));
        assert_eq!(parse_seconds("0:45"), Some(45.0));
    }

    #[test]
    fn formats_timecodes() {
        assert_eq!(format_timecode(0, 25.0), "00:00:00:00");
        assert_eq!(format_timecode(2262, 25.0), "00:01:30:12");
        assert_eq!(format_timecode(3723 * 25 + 24, 25.0), "01:02:03:24");
        assert_eq!(format_timecode(-5, 25.0), "00:00:00:00");
    }
}
