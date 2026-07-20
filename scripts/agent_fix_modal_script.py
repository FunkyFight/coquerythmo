from pathlib import Path

path = Path("scripts/agent_apply_language_modal.py")
text = path.read_text(encoding="utf-8")
old = 'text: "\\u{b}".into()'
new = 'text: "\\\\u{b}".into()'
if text.count(old) != 1:
    raise RuntimeError(f"expected one reverse-tab escape, found {text.count(old)}")
path.write_text(text.replace(old, new, 1), encoding="utf-8")
