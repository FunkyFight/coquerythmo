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


# Every editor entry point reads the active project band's setting. -----------
for path in [
    "src/workspaces/rythmo/mouse.rs",
    "src/workspaces/rythmo/mouse_buttons.rs",
    "src/workspaces/rythmo/press.rs",
]:
    text = read(path)
    old = "let lang = crate::config::get().lang.clone();"
    count = text.count(old)
    if count == 0:
        raise RuntimeError(f"{path}: no interface-locale syllable caller found")
    text = text.replace(old, "let lang = ctx.project.syllable_language_code();")
    text = text.replace("&lang,", "lang,")
    write(path, text)

path = "src/workspaces/rythmo/syllable.rs"
text = read(path)
text = replace_once(
    text,
    '''    let lang = crate::config::get().lang.clone();
    let ratios = syllable_ratios_for_line(line, state.syllable_drag.as_ref(), &lang, state)?;
''',
    '''    let lang = ctx.project.syllable_language_code();
    let ratios = syllable_ratios_for_line(line, state.syllable_drag.as_ref(), lang, state)?;
''',
    "syllable handle drag",
)
write(path, text)

path = "src/ui/mod.rs"
text = read(path)
text = replace_once(
    text,
    "                    let lang = crate::config::get().lang.clone();",
    "                    let lang = project.syllable_language_code();",
    "rendered text cursor hit testing",
)
text = text.replace("                        &lang,", "                        lang,")
write(path, text)

path = "src/workspaces/rythmo/view.rs"
text = read(path)
text = replace_once(
    text,
    "    let karaoke_lang = crate::config::get().lang.clone();",
    "    let karaoke_lang = project.syllable_language_code();",
    "workspace scene language",
)
text = text.replace("&karaoke_lang", "karaoke_lang")
write(path, text)

# CPU renderer consumes the backend-independent scene value. -----------------
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
if before.count(needle) != 2 or after.count(needle) != 2:
    raise RuntimeError(
        f"CPU locale references: expected 2 before/2 in helper, got {before.count(needle)}/{after.count(needle)}"
    )
before = before.replace(needle, "scene.syllable_language.code()")
after = after.replace(needle, "lang")
before = replace_once(
    before,
    "                    let lang = &crate::config::get().lang;",
    "                    let lang = scene.syllable_language.code();",
    "CPU segmented text language",
)
text = before + after
write(path, text)

# GPU renderer mirrors CPU and consumes exactly the same scene value. ---------
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
helper_region = text[helper_start:impl_start]
if helper_region.count(needle) != 2:
    raise RuntimeError(
        f"GPU helper locale references: expected 2, found {helper_region.count(needle)}"
    )
helper_region = helper_region.replace(needle, "lang")
text = text[:helper_start] + helper_region + text[impl_start:]
if text.count(needle) != 2:
    raise RuntimeError(f"GPU scene locale references: expected 2, found {text.count(needle)}")
text = text.replace(needle, "scene.syllable_language.code()")
text = replace_once(
    text,
    "                    let lang = &crate::config::get().lang;",
    "                    let lang = scene.syllable_language.code();",
    "GPU segmented text language",
)
write(path, text)

# Hard contract: project syllabification must never again follow the interface
# locale. Bootstrap remains the one legitimate consumer for UI translations.
remaining = []
for source in Path("src").rglob("*.rs"):
    for line_no, line in enumerate(source.read_text(encoding="utf-8").splitlines(), 1):
        if "config::get().lang" in line and source.as_posix() != "src/app/bootstrap.rs":
            remaining.append(f"{source}:{line_no}:{line}")
if remaining:
    raise RuntimeError(
        "interface locale still drives project syllabification:\n" + "\n".join(remaining)
    )
