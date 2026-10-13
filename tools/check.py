#!/usr/bin/env python3
"""Repository integrity and regression checks, with explicit optional tool reporting."""

import argparse
import ast
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent
# Standalone helpers share the translation runtime during source-tree tooling.
sys.path.insert(0, str(ROOT / "nacre/shell-tools"))
RETIRED = (
    "nacre/login/issue",
    "nacre/shell/run.fish",
    "waybar",
    "swaync",
    "waypaper",
    "wlogout",
    "matugen",
    "nwg-dock-hyprland",
    "rice",
    "nacre/wallpapers",
    "nacre/welcome",
    "hypr/scripts/low-battery.sh",
    "nacre/shell/utils/thumbnail.py",
    "nacre/shell/services/Cava.qml",
    "nacre/shell/widgets/Spectrum.qml",
    "nacre/shell/utils/scripts/beat.js",
    "nacre/shell-tools/tests/test_beats.mjs",
)


def run(command, env=None, stdout_only=False):
    result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, env=env)
    if result.returncode:
        raise RuntimeError(result.stdout + result.stderr)
    return result.stdout if stdout_only else result.stdout + result.stderr


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.parse_args()
    for path in RETIRED:
        if (ROOT / path).exists():
            raise RuntimeError("Retired desktop tree reintroduced: " + path)
    lock = json.loads((ROOT / "dependencies.lock.json").read_text())
    engine = ROOT / lock["orient"]["path"]
    if not (engine / "crates/orient-cli").is_dir():
        raise RuntimeError(
            "Initialize the pinned engine: git submodule update --init --recursive"
        )
    actual = run(["git", "-C", str(engine), "rev-parse", "HEAD"]).strip()
    if actual != lock["orient"]["revision"]:
        raise RuntimeError("Orient checkout differs from dependencies.lock.json")
    python_files = []
    for path in ROOT.rglob("*"):
        if not path.is_file() or any(
            part
            in (".git", "__pycache__", "build", "dist", ".venv", "node_modules", "deps")
            for part in path.parts
        ):
            continue
        if path.suffix == ".py" or (
            path.parent == ROOT / "bin"
            and path.read_text().startswith("#!/usr/bin/env python3")
        ):
            ast.parse(path.read_text(), filename=str(path))
            python_files.append(str(path))
        elif path.suffix == ".fish" and shutil.which("fish"):
            run(["fish", "--no-config", "-n", str(path)])
        elif path.suffix == ".json":
            json.loads(path.read_text())
        elif (
            path.suffix == ".sh"
            or path.parent == ROOT / "uwsm"
            or (
                path.parent == ROOT / "bin"
                and "bash" in path.read_text().splitlines()[0]
            )
        ):
            run(["bash", "-n", str(path)])
    ruff = shutil.which("ruff")
    if ruff:
        run([ruff, "format", "--target-version", "py313", "--check", *python_files])
        run([ruff, "check", *python_files])
        print("Python formatting and lint passed.", flush=True)
    elif os.environ.get("CI"):
        raise RuntimeError("CI requires the pinned Ruff formatter and linter")
    else:
        print("Ruff unavailable; Python formatting/lint checks skipped.", flush=True)
    print("Python, JSON, shell syntax and retired-tree checks passed.", flush=True)
    from window_rules import conflicts, bind_conflicts

    extra_lua = [ROOT / "nacre/shell-tools/shortcuts.lua"]
    contradictory = conflicts(ROOT / "hypr/conf", extra_lua) + bind_conflicts(
        ROOT / "hypr/conf", extra_lua
    )
    if contradictory:
        raise RuntimeError("Conflicting window rules:\n" + "\n".join(contradictory))
    print("Window class routing and floating rules are consistent.", flush=True)
    lua = shutil.which("luac")
    if lua:
        for path in [
            *(ROOT / "hypr").rglob("*.lua"),
            *extra_lua,
            *(ROOT / "tools").glob("*.lua"),
            ROOT / "nacre/shell-tools/compositor_backends/hyprland/bindings.lua",
        ]:
            run([lua, "-p", str(path)])
        print("Lua syntax passed.", flush=True)
    formatter = Path("/usr/lib/qt6/bin/qmlformat")
    if formatter.exists():
        for path in [*(ROOT / "nacre").rglob("*.qml"), *(ROOT / "nacre").rglob("*.js")]:
            formatted = run([str(formatter), str(path)], stdout_only=True)
            if formatted != path.read_text():
                raise RuntimeError(
                    "Qt source formatting differs: " + str(path.relative_to(ROOT))
                )
        print(
            "All desktop QML/JavaScript parsed and formatted with local Qt.", flush=True
        )
    else:
        print(
            "Desktop QML parser skipped; target-host validation remains required.",
            flush=True,
        )
    run([sys.executable, str(ROOT / "tools/generate_compositor_config.py"), "--check"])
    run([sys.executable, str(ROOT / "tools/compositor_boundary.py")])
    print("Compositor boundary and generated configuration checks passed.", flush=True)
    # Child suites must exercise the candidate engine, never the deployed old one.
    engine_source = str(ROOT / "nacre/shell-cli/src")
    os.environ["PYTHONPATH"] = (
        engine_source
        + os.pathsep
        + str(ROOT / "nacre/shell-tools")
        + os.pathsep
        + os.environ.get("PYTHONPATH", "")
    )
    for suite in (
        "tools/tests",
        "ai/tests",
        "brain/tests",
        "nacre/shell-tools/tests",
    ):
        output = run([sys.executable, "-m", "unittest", "discover", "-s", suite])
        summary = "\n".join(
            line
            for line in output.splitlines()
            if line.startswith(("Ran ", "OK ", "OK"))
        )
        print(suite + ": " + summary.replace("\n", "; "), flush=True)
    cli_python = Path(sys.executable)
    if cli_python.exists():
        environment = dict(os.environ, PYTHONPATH=str(ROOT / "nacre/shell-cli/src"))
        help_text = run(
            [str(cli_python), "-m", "nacre_shell", "--help"], env=environment
        )
        if "{scheme,wallpaper}" not in help_text:
            raise RuntimeError("Unexpected palette CLI commands")
        run(
            [str(cli_python), "-m", "nacre_shell", "scheme", "list", "--names"],
            env=environment,
        )
        print("Current palette CLI imported and parsed successfully.", flush=True)
    else:
        print(
            "Palette CLI smoke check skipped: install its pinned Python dependencies.",
            flush=True,
        )
    prettier = ROOT / "node_modules/prettier/bin/prettier.cjs"
    node = shutil.which("node")
    node_cmd = (
        [node]
        if node
        else (["nacre-ai-tools", "node"] if shutil.which("nacre-ai-tools") else [])
    )
    if node_cmd:
        run([*node_cmd, "--check", str(ROOT / "brain/web/app.js")])
        if prettier.is_file():
            run([*node_cmd, str(prettier), "--check", str(ROOT / "brain/web/app.js")])
            print("Brain JavaScript syntax and formatting passed.", flush=True)
        elif os.environ.get("CI"):
            raise RuntimeError("Install development formatter with npm ci")
        else:
            print(
                "Brain JavaScript syntax passed; formatter unavailable (npm ci).",
                flush=True,
            )
    else:
        print("Node.js checks skipped: Node.js unavailable.", flush=True)


if __name__ == "__main__":
    main()
