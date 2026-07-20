from pathlib import Path

path = Path("src/ui/language_modal.rs")
data = path.read_bytes()
replacements = {
    b'text: "\r".into()': b'text: "\\r".into()',
    b'text: "\t".into()': b'text: "\\t".into()',
}
for old, new in replacements.items():
    count = data.count(old)
    if count != 1:
        raise RuntimeError(f"expected one literal {old!r}, found {count}")
    data = data.replace(old, new, 1)
path.write_bytes(data)
