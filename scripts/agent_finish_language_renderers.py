from pathlib import Path


def read(path: str) -> str:
    return Path(path).read_text(encoding="utf-8")


def write(path: str, text: str) -> None:
    Path(path).write_text(text, encoding="utf-8")


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise RuntimeError(f"{label}: expected one occurrence, found {count}")
    return text.replace(old, new, 1)


# The controller/view half was applied before the first renderer assertion.
# Verify it is complete instead of replaying non-idempotent edits.
for path in [
    "src/workspaces/rythmo/mouse.rs",
    "src/workspaces/rythmo/mouse_buttons.rs",
    "src/workspaces/rythmo/press.rs",
    "src/workspaces/rythmo/syllable.rs",
    "src/workspaces/rythmo/view.rs",
    "src/ui/mod.rs",
]:
    if "crate::config::get().lang" in read(path):
        raise RuntimeError(f"{path}: project interaction still uses the interface locale")

# CPU renderer ---------------------------------------------------------------
path = "src/rythmo_cpu_renderer.rs"
text = read(path)
text = replace_once(
    text,
    '''fn blit_karaoke_dot(
    pixmap: &mut Pixmap,
    line: &crate::rythmo_line::RythmoLine,
    current_frame: f64,
''',
    '''fn blit_karaoke_dot(
    pixmap: &mut Pixmap,
    line: &crate::rythmo_line::RythmoLine,
    lang: &str,
    current_frame: f64,
''',
    "CPU karaoke dot signature",
)
text = replace_once(
    text,
    "                blit_karaoke_dot(&mut pixmap, line, current_frame as f64, x1, line_y, lw, s);",
    '''                blit_karaoke_dot(
                    &mut pixmap,
                    line,
                    scene.syllable_language.code(),
                    current_frame as f64,
                    x1,
                    line_y,
                    lw,
                    s,
                );''',
    "CPU karaoke dot call",
)
helper_index = text.index("fn blit_karaoke_dot(")
before = text[:helper_index]
after = text[helper_index:]
needle = "&crate::config::get().lang"
if before.count(needle) != 3 or after.count(needle) != 2:
    raise RuntimeError(
        f"CPU locale references: expected 3 before/2 in helper, got {before.count(needle)}/{after.count(needle)}"
    )
before = before.replace(needle, "scene.syllable_language.code()", 2)
before = replace_once(
    before,
    "                    let lang = &crate::config::get().lang;",
    "                    let lang = scene.syllable_language.code();",
    "CPU segmented text language",
)
after = after.replace(needle, "lang")
text = before + after
write(path, text)

# GPU renderer ---------------------------------------------------------------
path = "src/rythmo_gpu_renderer.rs"
text = read(path)
text = replace_once(
    text,
    '''fn push_karaoke_dot(
    quads: &mut Vec<QuadInstance>,
    line: &RythmoLine,
    current_frame: f64,
''',
    '''fn push_karaoke_dot(
    quads: &mut Vec<QuadInstance>,
    line: &RythmoLine,
    lang: &str,
    current_frame: f64,
''',
    "GPU karaoke dot signature",
)
text = replace_once(
    text,
    "                push_karaoke_dot(&mut quads, line, current_frame, x1, line_y, lw, s);",
    '''                push_karaoke_dot(
                    &mut quads,
                    line,
                    scene.syllable_language.code(),
                    current_frame,
                    x1,
                    line_y,
                    lw,
                    s,
                );''',
    "GPU karaoke dot call",
)
needle = "&crate::config::get().lang"
helper_start = text.index("fn push_karaoke_dot(")
impl_start = text.index("impl RythmoGpuRenderer", helper_start)
helper = text[helper_start:impl_start]
if helper.count(needle) != 2:
    raise RuntimeError(f"GPU helper locale references: expected 2, found {helper.count(needle)}")
helper = helper.replace(needle, "lang")
text = text[:helper_start] + helper + text[impl_start:]
if text.count(needle) != 3:
    raise RuntimeError(f"GPU scene locale references: expected 3, found {text.count(needle)}")
text = text.replace(needle, "scene.syllable_language.code()", 2)
text = replace_once(
    text,
    "                    let lang = &crate::config::get().lang;",
    "                    let lang = scene.syllable_language.code();",
    "GPU segmented text language",
)
write(path, text)

remaining = []
for source in Path("src").rglob("*.rs"):
    for line_no, line in enumerate(source.read_text(encoding="utf-8").splitlines(), 1):
        if "config::get().lang" in line and source.as_posix() != "src/app/bootstrap.rs":
            remaining.append(f"{source}:{line_no}:{line}")
if remaining:
    raise RuntimeError(
        "interface locale still drives project syllabification:\n" + "\n".join(remaining)
    )
