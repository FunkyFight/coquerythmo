"""Compares Needle 3 models on hand-written sentences, unseen in training.

Each sentence goes through the app's own tool selection (the Rust exporter),
then through `needle.exe --serve` with each model. A sentence counts as right
when the engine calls the expected tool (`call`) and, when given, with the
expected argument values. `held` counts calls the engine held back but named
correctly (the app runs those when the words point clearly at the tool).

Usage: python evaluate.py WORK_DIR ENGINE_DIR MODEL.cact [MODEL2.cact ...]
WORK_DIR receives eval_queries.jsonl, eval_rows.jsonl and the report.
"""

import json
import os
import socket
import subprocess
import sys
import time
import urllib.request

FACTS = {
    "date": "2026-10-15", "language": "fr", "playhead": "00:02:10:04", "fps": 25.0, "duration": "00:22:00:00",
    "selected": ["Bonjour tout le monde", "Marie", 0],
    "characters": ["Marie", "Jean", "Muffins", "Chanson", "Postière", "Le Narrateur"],
    "bands": ["VF", "EN"], "shown_band": "VF",
}

# (sentence, expected tool, expected arguments that must match, compared
# case-insensitively; None means no call is expected)
CASES = [
    ('Renomme le personnage "Muffins" en "Cup Cakes"', "rename_character", {"old_name": "Muffins", "new_name": "Cup Cakes"}),
    ("Va a 1 min 30", "seek_to_time", {"time": "1 min 30"}),
    ('Créé une phrase ligne 2 qui dit "Hello World"', "create_line", {"text": "Hello World", "track": 2}),
    ("Change le nom du personnage de la ligne en Cupcake", "rename_character", {"new_name": "Cupcake"}),
    ('Met la TOUTE PREMIERE LIGNE en gras, Celle qui dit "Postière Muffins."', "toggle_bold", {}),
    ("Positionne là au niveau de la première ligne de Chanson", "move_lines_to_character_line", {"character": "Chanson"}),
    ("tu peux aller à 3 minutes 10 ?", "seek_to_time", {"time": "3 minutes 10"}),
    ("on repart du tout début", "go_to_start", {}),
    ("lance", "play", {}),
    ("stoppe tout de suite la lecture", "pause", {}),
    ("le son est trop fort", "volume_down", {}),
    ("mets le son à 40", "set_volume", {"percent": 40}),
    ("coupe-moi le son", "mute", {}),
    ("pose une boucle ici", "add_loop", {}),
    ("il faut un changement de plan à cet endroit", "add_scene_change", {}),
    ("ajoute un point de sortie", "add_out_point", {}),
    ("une respiration ici", "add_breath", {}),
    ("crée une ligne pour Jean : On se retrouve demain", "create_line", {"character": "Jean", "text": "On se retrouve demain"}),
    ("nouvelle réplique de Postière sur la piste 3", "create_line", {"character": "Postière", "track": 3}),
    ("efface toutes les lignes de Muffins", "delete_lines", {"character": "Muffins"}),
    ("supprime cette réplique", "delete_selected_lines", {}),
    ("passe cette ligne en italique", "toggle_italic", {}),
    ("souligne-la", "toggle_underline", {}),
    ("c'est Jean qui dit cette phrase", "set_line_character", {"character": "Jean"}),
    ("déplace-la sur la piste 4", "move_lines_to_track", {"track": 4}),
    ("décale la réplique de 2 secondes", "shift_lines", {"seconds": 2}),
    ("la réplique doit durer 3 secondes", "set_line_duration", {"seconds": 3}),
    ("coupe cette réplique en deux", "split_line", {}),
    ("active le karaoké sur cette ligne", "toggle_karaoke", {}),
    ("mets de la colère dans la réplique", "set_text_emotion", {}),
    ("écris en note : trop long", "set_line_note", {"note": "trop long"}),
    ("cette réplique est hors champ", "set_line_off_screen", {}),
    ("annule ce que tu viens de faire", "undo", {}),
    ("Muffins en rouge", "set_character_color", {"character": "Muffins", "color": "rouge"}),
    ("quels personnages il y a ?", "list_characters", {}),
    ("combien de répliques pour Marie ?", "count_lines", {"character": "Marie"}),
    ("où est la réplique qui dit je reviens", "find_line", {}),
    ("on est à quel timecode ?", "get_playhead_time", {}),
    ("c'est quoi le raccourci pour mettre en gras ?", "get_shortcut", {}),
    ("je veux exporter la vidéo", "open_export", {}),
    ("exporte les sous-titres en SRT", "export_subtitles", {}),
    ("ouvre les réglages du projet", "open_project_settings", {}),
    ("change le style de la bande", "open_band_style", {}),
    ("affiche le panneau des rôles", "open_roles_panel", {}),
    ("enregistre le projet", "save_project", {}),
    ("passe sur la bande anglaise", "select_band", {}),
    ("ouvre les fichiers du projet", "toggle_project_files", {}),
    ("crée un comédien Paul Durand", "create_voice_actor", {"name": "Paul Durand"}),
    ("Rename Jean to Peter", "rename_character", {"old_name": "Jean", "new_name": "Peter"}),
    ("go to 2:45", "seek_to_time", {"time": "2:45"}),
    ("add a loop here", "add_loop", {}),
    ("make this line bold", "toggle_bold", {}),
    ("guarda el proyecto", "save_project", {}),
    ("raconte-moi une blague", None, {}),
]


def write_queries(path):
    with open(path, "w", encoding="utf-8") as out:
        for query, tool, arguments in CASES:
            answers = [{"name": tool, "arguments": arguments}] if tool else []
            out.write(json.dumps({"query": query, "answers": answers, "facts": FACTS}, ensure_ascii=False) + "\n")


def export_rows(repo, queries, rows):
    env = dict(os.environ, NEEDLE_TRAIN_IN=queries, NEEDLE_TRAIN_OUT=rows)
    subprocess.run(["cargo", "test", "--lib", "ai::training", "--", "--ignored"], cwd=repo, env=env,
                   check=True, capture_output=True)


def free_port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def ask(engine, model, row, work):
    tools_path = os.path.join(work, "eval_tools.json")
    system_path = os.path.join(work, "eval_system.txt")
    with open(tools_path, "w", encoding="utf-8") as handle:
        json.dump(row["tools"], handle, ensure_ascii=False)
    with open(system_path, "w", encoding="utf-8") as handle:
        handle.write(row["system"])
    port = free_port()
    process = subprocess.Popen([engine, "--model", model, "--tools", tools_path, "--system", system_path,
                                "--serve", "--port", str(port)], stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
    try:
        deadline = time.time() + 60
        while time.time() < deadline:
            line = process.stderr.readline().decode("utf-8", "replace")
            if "serving on" in line or not line:
                break
        request = urllib.request.Request("http://127.0.0.1:%d/complete" % port,
                                         data=json.dumps({"input": row["query"]}, ensure_ascii=False).encode("utf-8"),
                                         headers={"Content-Type": "application/json; charset=utf-8"})
        return json.load(urllib.request.urlopen(request, timeout=60))
    finally:
        process.kill()
        process.wait()


def same(expected, actual):
    if isinstance(expected, (int, float)) and isinstance(actual, (int, float)):
        return float(expected) == float(actual)
    return str(expected).strip().lower() == str(actual).strip().lower()


def judge(case, response):
    _, tool, arguments = case
    calls = response.get("function_calls") or []
    held = response.get("suppressed_calls") or []
    if tool is None:
        return ("right" if not calls else "wrong"), ""
    for kind, group in (("right", calls), ("held", held)):
        for call in group:
            if call.get("name") == tool:
                got = call.get("arguments") or {}
                bad = [name for name, value in arguments.items() if not same(value, got.get(name))]
                if bad:
                    return kind + "-args", json.dumps(got, ensure_ascii=False)
                return kind, ""
    names = [call.get("name") for call in calls] or ["(held) " + str(call.get("name")) for call in held]
    return ("wrong" if names else "none"), ", ".join(names)


def main():
    work, engine_dir, models = sys.argv[1], sys.argv[2], sys.argv[3:]
    repo = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    os.makedirs(work, exist_ok=True)
    queries = os.path.join(work, "eval_queries.jsonl")
    rows_path = os.path.join(work, "eval_rows.jsonl")
    write_queries(queries)
    export_rows(repo, queries, rows_path)
    rows = {json.loads(line)["query"]: json.loads(line) for line in open(rows_path, encoding="utf-8")}
    engine = os.path.join(engine_dir, "needle.exe")
    report = []
    for model in models:
        tally = {}
        for case in CASES:
            row = rows.get(case[0])
            if row is None:
                verdict, detail = "missed-by-app", ""
            else:
                verdict, detail = judge(case, ask(engine, model, row, work))
            tally[verdict] = tally.get(verdict, 0) + 1
            report.append("%-14s %-13s %s  %s" % (os.path.basename(model), verdict, case[0], detail))
        summary = "%s: %s" % (os.path.basename(model), ", ".join("%s %d" % item for item in sorted(tally.items())))
        report.append(summary)
        print(summary, flush=True)
    with open(os.path.join(work, "eval_report.txt"), "w", encoding="utf-8") as out:
        out.write("\n".join(report) + "\n")
    print("\n".join(line for line in report if not line.split()[1].startswith("right")))


if __name__ == "__main__":
    main()
