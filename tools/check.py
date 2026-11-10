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
import tempfile

ROOT = Path(__file__).resolve().parent.parent
# Standalone helpers share the translation runtime during source-tree tooling.
sys.path.insert(0, str(ROOT / "nacre/shell-tools"))
RETIRED = (
    "nacre/shell",
    "nacre/shell-tools/tests/qml_source.py",
    "nacre/login/issue",
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
    "nacre/shell-tools/tests/test_beats.mjs",
)


def run(command, env=None, stdout_only=False):
    result = subprocess.run(
        command,
        cwd=ROOT,
        capture_output=True,
        text=True,
        env=env,
        stdin=subprocess.DEVNULL,
    )
    if result.returncode:
        raise RuntimeError(result.stdout + result.stderr)
    return result.stdout if stdout_only else result.stdout + result.stderr


def test_environment():
    """Exercise checkout fixtures independently of the running desktop session."""
    environment = dict(os.environ)
    # Runtime HOME is isolated below; retain the compiler cache for source-build
    # fixtures instead of refetching the pinned Bead graph into every fake HOME.
    environment.setdefault("CARGO_HOME", str(Path.home() / ".cargo"))
    environment.setdefault("CARGO_BUILD_JOBS", "2")
    for key in (
        "NACRE_COMPOSITOR",
        "NACRE_COMPOSITOR_TOOLS",
        "NACRE_PEARL_TOOLS",
        "NIRI_SOCKET",
        "NIRI_CONFIG",
        "NACRE_NIRI_CONFIG",
        "NACRE_NIRI_DESKTOP_CONFIG",
        "HYPRLAND_INSTANCE_SIGNATURE",
        "XDG_CURRENT_DESKTOP",
        "WAYLAND_DISPLAY",
        "QT_LOGGING_RULES",
        "QML_IMPORT_PATH",
        "QML2_IMPORT_PATH",
    ):
        environment.pop(key, None)
    environment["QT_QPA_PLATFORM"] = "offscreen"
    # CI's root process reads a runner-owned checkout. Keep trust scoped to this
    # workspace and its pinned submodule when the disposable HOME hides the
    # runner's git config.
    count = int(environment.get("GIT_CONFIG_COUNT", "0"))
    engine_path = json.loads((ROOT / "dependencies.lock.json").read_text())["orient"][
        "path"
    ]
    for offset, path in enumerate((ROOT, ROOT / engine_path)):
        environment[f"GIT_CONFIG_KEY_{count + offset}"] = "safe.directory"
        environment[f"GIT_CONFIG_VALUE_{count + offset}"] = str(path)
    environment["GIT_CONFIG_COUNT"] = str(count + 2)
    environment["PYTHONPATH"] = os.pathsep.join(
        (
            str(ROOT / "nacre/shell-cli/src"),
            str(ROOT / "nacre/shell-tools"),
            environment.get("PYTHONPATH", ""),
        )
    )
    return environment


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.parse_args()
    run([sys.executable, str(ROOT / "tools/generate_usage_keybindings.py"), "--check"])
    print("Shortcut documentation matches the compositor contract.", flush=True)
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
        if (
            not path.is_file()
            or path.is_relative_to(ROOT / "bead-shell")
            or path.is_relative_to(ROOT / "vendor")
            or path.is_relative_to(ROOT / ".lead")
            or any(
                part
                in (
                    ".git",
                    "__pycache__",
                    "build",
                    "dist",
                    ".venv",
                    ".research",
                    "node_modules",
                    "deps",
                    "target",
                )
                for part in path.parts
            )
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
            # The injected Spotify asset is browser JavaScript, checked with Node below.
            if path == ROOT / "nacre/shell-tools/spicetify-theme.js":
                continue
            formatted = run([str(formatter), str(path)], stdout_only=True)
            if formatted != path.read_text():
                raise RuntimeError(
                    "Qt source formatting differs: " + str(path.relative_to(ROOT))
                )
        print(
            "SDDM greeter QML/JavaScript parsed and formatted with local Qt.",
            flush=True,
        )
    else:
        print(
            "SDDM greeter QML parser unavailable; target-host validation remains required.",
            flush=True,
        )
    run([sys.executable, str(ROOT / "tools/generate_compositor_config.py"), "--check"])
    run([sys.executable, str(ROOT / "tools/compositor_boundary.py")])
    print("Compositor boundary and generated configuration checks passed.", flush=True)
    # Child suites must exercise the candidate engine, never the deployed old one.
    environment = test_environment()
    for suite in (
        "tools/tests",
        "ai/tests",
        "brain/tests",
        "nacre/shell-tools/tests",
    ):
        output = run(
            [sys.executable, "-m", "unittest", "discover", "-s", suite],
            env=environment,
        )
        summary = "\n".join(
            line
            for line in output.splitlines()
            if line.startswith(("Ran ", "OK ", "OK"))
        )
        print(suite + ": " + summary.replace("\n", "; "), flush=True)
    product_output = run(
        [sys.executable, "tools/check_product_shell.py", "--python-only"],
        env=environment,
    )
    for line in product_output.splitlines():
        if line.startswith(
            (
                "Bead product suite ",
                "Bead product proof suites:",
                "Bead product Python tests:",
            )
        ):
            print(line, flush=True)
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
        graph_script = ROOT / "brain/web/graph.js"
        graph_browser_test = ROOT / "brain/tests/browser-graph.mjs"
        ask_browser_test = ROOT / "brain/tests/browser-ask.mjs"
        if graph_script.is_file():
            run([*node_cmd, "--check", str(graph_script)])
        if graph_browser_test.is_file():
            run([*node_cmd, "--check", str(graph_browser_test)])
        if ask_browser_test.is_file():
            run([*node_cmd, "--check", str(ask_browser_test)])
        vendor_graph = ROOT / "brain/web/vendor/d3-force.min.js"
        if vendor_graph.is_file():
            run([*node_cmd, "--check", str(vendor_graph)])
        spotify_script = ROOT / "nacre/shell-tools/spicetify-theme.js"
        run([*node_cmd, "--check", str(spotify_script)])
        if prettier.is_file():
            run([*node_cmd, str(prettier), "--check", str(ROOT / "brain/web/app.js")])
            if graph_script.is_file():
                run([*node_cmd, str(prettier), "--check", str(graph_script)])
            if graph_browser_test.is_file():
                run([*node_cmd, str(prettier), "--check", str(graph_browser_test)])
            if ask_browser_test.is_file():
                run([*node_cmd, str(prettier), "--check", str(ask_browser_test)])
            print("Brain JavaScript syntax and formatting passed.", flush=True)
            run([*node_cmd, str(prettier), "--check", str(spotify_script)])
            print("Spicetify JavaScript syntax and formatting passed.", flush=True)
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
    # Static config analyses also need checkout routing. This changes only the
    # checker process, leaving the invoking desktop/deployment environment intact.
    with tempfile.TemporaryDirectory(prefix="nacre-check-home-") as folder:
        isolated = test_environment()
        isolated.update(
            HOME=folder,
            XDG_CONFIG_HOME=str(Path(folder) / "config"),
            XDG_CACHE_HOME=str(Path(folder) / "cache"),
            XDG_STATE_HOME=str(Path(folder) / "state"),
            XDG_DATA_HOME=str(Path(folder) / "data"),
        )
        os.environ.clear()
        os.environ.update(isolated)
        main()
