from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]

subprocess.run(
    [
        "sudo",
        "apt-get",
        "install",
        "-y",
        "libasound2-dev",
        "libudev-dev",
        "pkg-config",
    ],
    cwd=ROOT,
    check=True,
)

result = subprocess.run(
    ["cargo", "test", "--locked", "detection", "--", "--nocapture"],
    cwd=ROOT,
    text=True,
    stdout=subprocess.PIPE,
    stderr=subprocess.STDOUT,
)
print(result.stdout)
if result.returncode == 0:
    print("detection editor v2 targeted tests pass")
    raise SystemExit(0)

log_path = ROOT / "tools" / "detection-v2-test.log"
log_path.write_text(result.stdout, encoding="utf-8")
subprocess.run(["git", "config", "user.name", "github-actions[bot]"], cwd=ROOT, check=True)
subprocess.run(
    ["git", "config", "user.email", "41898282+github-actions[bot]@users.noreply.github.com"],
    cwd=ROOT,
    check=True,
)
subprocess.run(["git", "add", str(log_path.relative_to(ROOT))], cwd=ROOT, check=True)
subprocess.run(
    ["git", "commit", "-m", "Capture detection v2 targeted test failure"],
    cwd=ROOT,
    check=True,
)
subprocess.run(
    ["git", "push", "origin", "HEAD:agent/track-scoped-detection-model"],
    cwd=ROOT,
    check=True,
)
sys.exit(result.returncode)
