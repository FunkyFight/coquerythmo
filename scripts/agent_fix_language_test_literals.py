from pathlib import Path

path = Path("src/ui/language_modal.rs")
data = path.read_bytes()

enter_variants = [
    b'&UiEvent::KeyInput { text: "\r\n".into() },',
    b'&UiEvent::KeyInput { text: "\r".into() },',
    b'&UiEvent::KeyInput { text: "\n".into() },',
]
enter_matches = [(value, data.count(value)) for value in enter_variants]
if sum(count for _, count in enter_matches) != 1:
    raise RuntimeError(f"expected one generated enter literal, found {enter_matches!r}")
for old, count in enter_matches:
    if count == 1:
        data = data.replace(
            old,
            b'&UiEvent::KeyInput { text: "\\r".into() },',
            1,
        )
        break

tab_literal = b'&UiEvent::KeyInput { text: "\t".into() },'
if data.count(tab_literal) != 1:
    raise RuntimeError(f"expected one generated tab literal, found {data.count(tab_literal)}")
data = data.replace(
    tab_literal,
    b'&UiEvent::KeyInput { text: "\\t".into() },',
    1,
)

path.write_bytes(data)
