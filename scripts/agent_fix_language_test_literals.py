from pathlib import Path

path = Path("src/ui/language_modal.rs")
data = path.read_bytes()

# The patch generator inserted the control characters themselves inside Rust
# string literals. Normalize those exact test call sites to escaped source.
replacements = {
    b'&UiEvent::KeyInput { text: "\n".into() },': b'&UiEvent::KeyInput { text: "\\r".into() },',
    b'&UiEvent::KeyInput { text: "\t".into() },': b'&UiEvent::KeyInput { text: "\\t".into() },',
}
for old, new in replacements.items():
    count = data.count(old)
    if count != 1:
        raise RuntimeError(f"expected one generated control literal {old!r}, found {count}")
    data = data.replace(old, new, 1)

path.write_bytes(data)
