from pathlib import Path

path = Path("src/workspaces/rythmo/detection_ui.rs")
text = path.read_text(encoding="utf-8")
old = """            if next != state.detection_hover {
                state.detection_hover = next;
                return Some(EventResponse::Consumed);
            }
"""
new = """            if next != state.detection_hover {
                state.detection_hover = next;
            }
            // Keep propagating ordinary pointer movement so the established
            // line-hover state and cursor feedback remain in sync with the
            // detection preview.
"""
if text.count(old) != 1:
    raise SystemExit(f"expected one detection hover block, found {text.count(old)}")
path.write_text(text.replace(old, new, 1), encoding="utf-8", newline="\n")
print("Detection hover propagation fixed")
