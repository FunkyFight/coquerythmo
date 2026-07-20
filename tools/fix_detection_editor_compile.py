from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def read(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def write(path: str, text: str) -> None:
    (ROOT / path).write_text(text, encoding="utf-8")


# Keep the feature self-contained: no lockfile change is needed for Unicode
# grapheme-ish boundaries used by the editor. The helper covers combining marks,
# variation selectors, emoji modifiers and ZWJ sequences.
cargo = read("Cargo.toml")
cargo = cargo.replace('\nunicode-segmentation = "1.12"\n', '\n')
write("Cargo.toml", cargo)

path = "src/detection.rs"
text = read(path).replace("use unicode_segmentation::UnicodeSegmentation;\n", "")
text = text.replace("let grapheme_count = text.graphemes(true).count();", "let grapheme_count = grapheme_count(text);")
old = '''            let char_boundary = text
                .graphemes(true)
                .take(point.grapheme_boundary as usize)
                .map(str::chars)
                .map(Iterator::count)
                .sum::<usize>();'''
text = text.replace(
    old,
    "            let char_boundary = char_boundary_for_grapheme(text, point.grapheme_boundary as usize);",
)
insert_after = '''    pub fn line_sync_mut(&mut self, line_id: u64) -> &mut LineSyncData {
        self.lines.entry(line_id).or_default()
    }
'''
addition = insert_after + '''
    /// Replace only source-video detections while preserving this language's
    /// dialogue synchronization data.
    pub fn replace_source_tracks_from(&mut self, source: &Self) {
        self.tracks = source.tracks.clone();
    }
'''
if "replace_source_tracks_from" not in text:
    if insert_after not in text:
        raise SystemExit("DetectionDocument insertion point missing")
    text = text.replace(insert_after, addition, 1)
helper_marker = "fn normalize_positive(values: &mut [f32]) {\n"
helpers = r'''pub fn grapheme_count(text: &str) -> usize {
    grapheme_char_boundaries(text).len().saturating_sub(1)
}

pub fn char_boundary_for_grapheme(text: &str, boundary: usize) -> usize {
    grapheme_char_boundaries(text)
        .get(boundary)
        .copied()
        .unwrap_or_else(|| text.chars().count())
}

fn grapheme_char_boundaries(text: &str) -> Vec<usize> {
    let mut boundaries = vec![0usize];
    let mut char_index = 0usize;
    let mut join_next = false;
    for character in text.chars() {
        let continuation = char_index > 0
            && (join_next
                || is_combining_mark(character)
                || is_variation_selector(character)
                || is_emoji_modifier(character)
                || character == '\u{200d}');
        if !continuation && char_index > 0 {
            boundaries.push(char_index);
        }
        join_next = character == '\u{200d}';
        char_index += 1;
    }
    if boundaries.last().copied() != Some(char_index) {
        boundaries.push(char_index);
    }
    boundaries
}

fn is_combining_mark(character: char) -> bool {
    matches!(
        character as u32,
        0x0300..=0x036f
            | 0x0483..=0x0489
            | 0x0591..=0x05bd
            | 0x05bf
            | 0x05c1..=0x05c2
            | 0x05c4..=0x05c5
            | 0x0610..=0x061a
            | 0x064b..=0x065f
            | 0x0670
            | 0x06d6..=0x06ed
            | 0x0900..=0x0903
            | 0x093a..=0x094f
            | 0x0981..=0x0983
            | 0x09bc..=0x09cd
            | 0x0a01..=0x0a03
            | 0x0a3c..=0x0a4d
            | 0x0b01..=0x0b03
            | 0x0b3c..=0x0b4d
            | 0x0c00..=0x0c04
            | 0x0c3e..=0x0c56
            | 0x0d00..=0x0d03
            | 0x0d3b..=0x0d4d
            | 0x0e31
            | 0x0e34..=0x0e3a
            | 0x0e47..=0x0e4e
            | 0x0f71..=0x0f84
            | 0x102b..=0x103e
            | 0x17b4..=0x17d3
            | 0x1ab0..=0x1aff
            | 0x1dc0..=0x1dff
            | 0x20d0..=0x20ff
            | 0xfe20..=0xfe2f
    )
}

fn is_variation_selector(character: char) -> bool {
    matches!(character as u32, 0xfe00..=0xfe0f | 0xe0100..=0xe01ef)
}

fn is_emoji_modifier(character: char) -> bool {
    matches!(character as u32, 0x1f3fb..=0x1f3ff)
}

'''
if "pub fn grapheme_count" not in text:
    if helper_marker not in text:
        raise SystemExit("grapheme helper insertion point missing")
    text = text.replace(helper_marker, helpers + helper_marker, 1)
write(path, text)

path = "src/workspaces/rythmo/detection_ui.rs"
text = read(path).replace("use unicode_segmentation::UnicodeSegmentation;\n", "")
text = text.replace("line.text.graphemes(true).count()", "crate::detection::grapheme_count(&line.text)")
write(path, text)

print("detection editor v2 compile compatibility fixes applied")
