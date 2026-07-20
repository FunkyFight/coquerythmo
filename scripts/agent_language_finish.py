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


# Modal host: preserve the existing modal routing while exposing the precise
# role/value for the new choice control.
path = "src/ui/modal_host.rs"
text = read(path)
old_focus = '''            if let Some(label) = self
                .languages
                .as_ref()
                .map(|modal| modal.keyboard_focus_label())
            {
                return ModalOutcome::Action(UiAction::Accessibility(
                    crate::accessibility::AccessibilityEvent::Focus {
                        label,
                        role: "control".to_string(),
                    },
                ));
            }
'''
new_focus = '''            if let Some((label, role)) = self.languages.as_ref().map(|modal| {
                (
                    modal.keyboard_focus_label(),
                    modal.keyboard_focus_role().to_string(),
                )
            }) {
                return ModalOutcome::Action(UiAction::Accessibility(
                    crate::accessibility::AccessibilityEvent::Focus { label, role },
                ));
            }
'''
text = replace_once(text, old_focus, new_focus, "language modal focus role")
text = replace_once(
    text,
    '''            LanguageModalResult::Select { id } => {
                ModalOutcome::Action(UiAction::SelectLanguage { id })
            }
            LanguageModalResult::PickInstrumental { id } => {
''',
    '''            LanguageModalResult::Select { id } => {
                ModalOutcome::Action(UiAction::SelectLanguage { id })
            }
            LanguageModalResult::SetSyllableLanguage { id, language } => {
                ModalOutcome::Actions(vec![
                    UiAction::SetLanguageSyllableLanguage { id, language },
                    UiAction::Accessibility(
                        crate::accessibility::AccessibilityEvent::ValueChanged {
                            label: crate::i18n::t("languages.syllables").to_string(),
                            value: super::language_modal::syllable_language_label(language)
                                .to_string(),
                        },
                    ),
                ])
            }
            LanguageModalResult::PickInstrumental { id } => {
''',
    "language modal result",
)
write(path, text)

# Editing/controller callers: all pointer and keyboard paths read the active
# project language rather than the application locale.
for path in [
    "src/workspaces/rythmo/mouse.rs",
    "src/workspaces/rythmo/mouse_buttons.rs",
    "src/workspaces/rythmo/press.rs",
]:
    text = read(path)
    count = text.count("let lang = crate::config::get().lang.clone();")
    if count == 0:
        raise RuntimeError(f"{path}: expected at least one UI-language caller")
    text = text.replace(
        "let lang = crate::config::get().lang.clone();",
        "let lang = ctx.project.syllable_language_code();",
    )
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
    "syllable drag caller",
)
write(path, text)

path = "src/ui/mod.rs"
text = read(path)
text = replace_once(
    text,
    "                    let lang = crate::config::get().lang.clone();",
    "                    let lang = project.syllable_language_code();",
    "GPU cursor hit testing",
)
text = text.replace("                        &lang,", "                        lang,")
write(path, text)

path = "src/workspaces/rythmo/view.rs"
text = read(path)
text = replace_once(
    text,
    "    let karaoke_lang = crate::config::get().lang.clone();",
    "    let karaoke_lang = project.syllable_language_code();",
    "workspace render language",
)
text = text.replace("&karaoke_lang", "karaoke_lang")
write(path, text)

# CPU renderer. The shared scene is the backend-independent source of truth,
# and the dot helper receives the same value explicitly.
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
# First two direct references are in the main scene loop; the final two are in
# the helper we just parameterized.
needle = "&crate::config::get().lang"
positions = [i for i in range(len(text)) if text.startswith(needle, i)]
if len(positions) != 4:
    raise RuntimeError(f"CPU renderer: expected four direct locale references, found {len(positions)}")
for _ in range(2):
    text = text.replace(needle, "scene.syllable_language.code()", 1)
text = replace_once(
    text,
    "                    let lang = &crate::config::get().lang;",
    "                    let lang = scene.syllable_language.code();",
    "CPU segmented line language",
)
text = text.replace(needle, "lang")
if "crate::config::get().lang" in text:
    raise RuntimeError("CPU renderer still depends on UI locale")
write(path, text)

# GPU renderer mirrors the CPU path and consumes the same scene field.
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
positions = [i for i in range(len(text)) if text.startswith(needle, i)]
if len(positions) != 4:
    raise RuntimeError(f"GPU renderer: expected four direct locale references, found {len(positions)}")
for _ in range(2):
    text = text.replace(needle, "lang", 1)
for _ in range(2):
    text = text.replace(needle, "scene.syllable_language.code()", 1)
text = replace_once(
    text,
    "                    let lang = &crate::config::get().lang;",
    "                    let lang = scene.syllable_language.code();",
    "GPU segmented line language",
)
if "crate::config::get().lang" in text:
    raise RuntimeError("GPU renderer still depends on UI locale")
write(path, text)

# Localized labels for the new accessible value control.
translations = {
    "i18n/fr.toml": (
        '"languages.clear_instrumental" = "Retirer l’instrumental"',
        '"languages.clear_instrumental" = "Retirer l’instrumental"\n"languages.syllables" = "Langue de découpe des syllabes"\n"languages.syllables.french" = "Français"\n"languages.syllables.english" = "Anglais"',
    ),
    "i18n/en.toml": (
        '"languages.clear_instrumental" = "Remove instrumental"',
        '"languages.clear_instrumental" = "Remove instrumental"\n"languages.syllables" = "Syllable language"\n"languages.syllables.french" = "French"\n"languages.syllables.english" = "English"',
    ),
    "i18n/es.toml": (
        '"languages.clear_instrumental" = "Quitar instrumental"',
        '"languages.clear_instrumental" = "Quitar instrumental"\n"languages.syllables" = "Idioma de separación silábica"\n"languages.syllables.french" = "Francés"\n"languages.syllables.english" = "Inglés"',
    ),
}
for path, (old, new) in translations.items():
    text = read(path)
    text = replace_once(text, old, new, f"translations in {path}")
    write(path, text)

# Only bootstrap is allowed to use the interface locale after this change.
remaining = []
for source in Path("src").rglob("*.rs"):
    for line_no, line in enumerate(source.read_text(encoding="utf-8").splitlines(), 1):
        if "config::get().lang" in line and source.as_posix() != "src/app/bootstrap.rs":
            remaining.append(f"{source}:{line_no}:{line}")
if remaining:
    raise RuntimeError("UI locale still drives project syllabification:\n" + "\n".join(remaining))
