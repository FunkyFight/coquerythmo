from pathlib import Path
import runpy

ROOT = Path(__file__).resolve().parents[1]
runpy.run_path(str(ROOT / "tools" / "finish_detection_editor_v2.py"), run_name="__main__")
