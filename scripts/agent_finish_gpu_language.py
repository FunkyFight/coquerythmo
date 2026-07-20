from pathlib import Path

path = Path("src/rythmo_gpu_renderer.rs")
text = path.read_text(encoding="utf-8")


def replace_once(old: str, new: str, label: str) -> None:
    global text
    count = text.count(old)
    if count != 1:
        raise RuntimeError(f"{label}: expected one occurrence, found {count}")
    text = text.replace(old, new, 1)


replace_once(
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
replace_once(
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
impl_start = text.index("impl GpuRenderer", helper_start)
helper = text[helper_start:impl_start]
if helper.count(needle) != 2:
    raise RuntimeError(f"GPU helper locale references: expected 2, found {helper.count(needle)}")
helper = helper.replace(needle, "lang")
text = text[:helper_start] + helper + text[impl_start:]
if text.count(needle) != 3:
    raise RuntimeError(f"GPU scene locale references: expected 3, found {text.count(needle)}")
text = text.replace(needle, "scene.syllable_language.code()", 2)
replace_once(
    "                    let lang = &crate::config::get().lang;",
    "                    let lang = scene.syllable_language.code();",
    "GPU segmented text language",
)
path.write_text(text, encoding="utf-8")

remaining = []
for source in Path("src").rglob("*.rs"):
    for line_no, line in enumerate(source.read_text(encoding="utf-8").splitlines(), 1):
        if "config::get().lang" in line and source.as_posix() != "src/app/bootstrap.rs":
            remaining.append(f"{source}:{line_no}:{line}")
if remaining:
    raise RuntimeError(
        "interface locale still drives project syllabification:\n" + "\n".join(remaining)
    )
