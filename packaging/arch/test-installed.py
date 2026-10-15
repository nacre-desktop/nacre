"""Installed-package acceptance; run only as a fresh nacre-package-test* user.

This deliberately refuses existing configuration/account directories. Use a
VM/disposable runtime root, never a maintainer's account.
"""

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--compositor", choices=("hyprland", "niri", "both"), required=True)
backend = parser.parse_args().compositor
home = Path.home()
assert (
    os.geteuid() != 0
    and home.parent == Path("/home")
    and home.name.startswith("nacre-package-test")
), home
for tool in ("gcc", "cc", "g++", "npm", "node", "ruff", "git", "pip"):
    assert shutil.which(tool) is None, tool
print("Runtime has no compiler, Node/npm, Ruff, Git or pip.")


def run(*args, success=True):
    result = subprocess.run(args, text=True, capture_output=True)
    if (result.returncode == 0) != success:
        print("\n".join((result.stdout + result.stderr).splitlines()[-20:]))
        raise RuntimeError(f"Unexpected exit {result.returncode}: {args}")
    return result


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


assert not (home / ".config/nacre").exists(), "Use a fresh disposable user"
assert not (home / ".codex").exists(), "Do not use an account with credentials"
# Root-owned ELF and its compatibility link are inside the package trust boundary.
binary = Path("/usr/lib/nacre/nacre-helper")
reference = Path("/usr/share/nacre/nacre/shell-tools/nacre-helper")
assert reference.is_symlink() and reference.resolve(strict=True) == binary
assert not binary.is_symlink() and binary.read_bytes()[:4] == b"\x7fELF"
for target in (binary, reference, *binary.parents, *reference.parents):
    metadata = target.lstat()
    assert metadata.st_uid == 0, target
    if not target.is_symlink():
        assert metadata.st_mode & 0o022 == 0, target
assert run("pacman", "-Qoq", str(binary)).stdout.strip() == "nacre"
for name in ("nacre-daemon", "nacrectl"):
    alias = Path("/usr/bin") / name
    assert alias.is_symlink() and alias.resolve(strict=True) == binary
    assert alias.lstat().st_uid == 0
    for parent in alias.parents:
        assert parent.stat().st_uid == 0 and parent.stat().st_mode & 0o022 == 0
    assert run("pacman", "-Qoq", str(alias)).stdout.strip() == "nacre"
assert "nacrectl status" in run("nacrectl", "--help").stdout
assert run("pacman", "-Qoq", "/usr/bin/orient").stdout.strip() == "orient"
assert run("pacman", "-Qoq", "/usr/bin/nacre_shell").stdout.strip() == "nacre"
assert importlib.util.find_spec("nacre_shell") is None, (
    "Palette Python SDK is developer-only"
)
assert not Path("/usr/share/nacre/runtime/palette").exists()
metadata = json.loads(Path("/usr/share/nacre/package.json.installed").read_text())
engine = metadata["orient"]["engine"]
assert run("orient", "--version").stdout.strip() == engine
assert run("nacre_shell", "--version").stdout.strip() == engine
for package in (
    "fontconfig",
    "freetype2",
    "libraqm",
    "libjpeg-turbo",
    "lcms2",
    "librsvg",
):
    run("pacman", "-Q", package)
for name in (
    "LICENSE-Orient",
    "LICENSE-Pillow",
    "NOTICE-Orient",
    "libjpeg-turbo-LICENSE.md",
):
    notice = Path("/usr/share/licenses/nacre") / name
    assert notice.is_file() and notice.stat().st_uid == 0
# The public native CLI must work before setup creates private runtime links.
wallpaper = Path("/usr/share/nacre/nacre/shell-tools/demo-wallpapers/forest.jpg")
query = json.loads(run("nacre_shell", "wallpaper", "--print", str(wallpaper)).stdout)
assert query["engine"] == engine and query["colours"]
assert not (home / ".local/state/nacre").exists(), "Read-only native query wrote state"
print(
    "Native palette CLI and independently packaged Orient match the engine lock before setup."
)
# Simulate a distro-configured plain Niri/Noctalia session, kept byte-for-byte.
plain = home / ".config/niri/config.kdl"
plain.parent.mkdir(parents=True)
plain.write_text("// Existing distribution session fixture\n")
plain_hash = digest(plain)
base = (
    "nacre",
    "setup",
    "--non-interactive",
    "--profile",
    "default",
    "--compositor",
    "hyprland" if backend == "hyprland" else "niri",
    "--layout",
    "no",
    "--with",
    "ai",
    "--with",
    "brain",
)
if backend == "both":
    base += ("--also-compositor", "hyprland")
os.environ["NACRE_COMPOSITOR"] = "hyprland" if backend == "hyprland" else "niri"
run(*base, "--plan")
assert not (home / ".config/nacre").exists()
account = home / ".codex/account-fixture"
account.parent.mkdir()
account.write_text("fresh fixture, not credentials")
run(*base, "--yes")
status = json.loads(run("nacre", "status").stdout)
assert status["status"] == "prepared", status
profile = json.loads((home / ".config/nacre/profile.json").read_text())
assert profile["aiFullAccess"] is False and {"ai", "brain", "configs", "shell"} <= set(
    profile["components"]
)
keyboard = home / ".config/nacre/keyboard.lua"
assert '"no"' in keyboard.read_text()
assert '["input.kb_layout"] = "no"' in keyboard.read_text(), (
    "Hyprland must receive Lua even when setup runs from Niri"
)
keyboard_hash = digest(keyboard)
for relative in (
    ".config/quickshell/nacre",
    ".local/share/nacre/shell/tools",
    ".local/share/nacre/brain",
    ".local/share/nacre/ai/application",
    ".config/systemd/user/timers.target.wants/nacre-brain-check.timer",
):
    path = home / relative
    assert path.is_symlink() and path.exists(), relative
assert not (
    home / ".config/systemd/user/graphical-session.target.wants/nacre-shell.service"
).exists()
assert digest(plain) == plain_hash
for name in ("nacre-daemon", "nacrectl"):
    private = home / ".local/share/nacre/shell/bin" / name
    public = home / ".local/bin" / name
    assert private.is_symlink() and private.resolve(strict=True) == binary
    assert public.is_symlink() and public.resolve(strict=True) == binary
for name in ("nacre-daemon.socket", "nacre-daemon.service"):
    assert (home / ".config/systemd/user" / name).is_file()
assert not any(
    "nacre-daemon" in path.name
    for directory in (home / ".config/systemd/user").glob("*.wants")
    for path in directory.iterdir()
), "Package setup must not enable foundation units"
print("Native daemon/controller aliases and manual-only unit templates are prepared.")
if backend in {"niri", "both"}:
    sys.path.insert(0, "/usr/share/nacre/tools")
    from package_niri import validate, command

    validate(home / ".config/nacre/niri/config.kdl")
    launch = command(home / ".config/nacre/niri/config.kdl")
    parsed = subprocess.run(
        [*launch[:2], "-n", *launch[2:]], text=True, capture_output=True, timeout=15
    )
    assert "unrecognized arguments" not in parsed.stderr, parsed.stderr
    assert "Command Line:" in parsed.stdout and "--session --config" in parsed.stdout, (
        parsed.stdout + parsed.stderr
    )
    assert Path("/usr/share/wayland-sessions/nacre-niri.desktop").is_file()
    assert (home / ".config/systemd/user/nacre-niri.target").is_file()
    appearance = (home / ".config/nacre/niri/desktop.kdl").read_text()
    assert "focus-ring { width 3; }" in appearance
    assert "border { width 3; }" in appearance
    assert "geometry-corner-radius 10" in appearance
    assert "active-gradient" in (home / ".config/nacre/niri/palette.kdl").read_text()
    startup = home / ".config/systemd/user/nacre-niri-startup.service"
    assert startup.is_file()
    assert (
        '"nacre-niri-startup.service"'
        in (home / ".config/nacre/niri/config.kdl").read_text()
    )
    with tempfile.TemporaryDirectory(prefix="nacre-unit-verify-") as runtime:
        verified = subprocess.run(
            ["systemd-analyze", "--user", "verify", str(startup)],
            env={**os.environ, "XDG_RUNTIME_DIR": runtime},
            text=True,
            capture_output=True,
        )
        assert verified.returncode == 0, verified.stderr
if backend in {"hyprland", "both"}:
    assert Path("/usr/share/wayland-sessions/nacre-hyprland.desktop").is_file()
    assert (home / ".config/systemd/user/nacre-hyprland.target").is_file()
wallpaper = Path((home / ".local/state/nacre/wallpaper/last.txt").read_text().strip())
assert wallpaper.is_file() and wallpaper.suffix == ".jpg"
presentation = json.loads((home / ".local/state/nacre/presentation.json").read_text())
assert presentation["poster"] == str(wallpaper) and presentation["colours"]
render = subprocess.run(
    [str(binary), "palette-api", "lock-dashboard", "publish"],
    input=json.dumps(
        {
            "args": [
                {"preferences": {}, "notifications": []},
                presentation["colours"],
                str(wallpaper),
                None,
                {"$path": str(home)},
                [],
            ],
            "kwargs": {},
        }
    ),
    text=True,
    capture_output=True,
    timeout=120,
)
assert render.returncode == 0, render.stderr + render.stdout
assert "result" in json.loads(render.stdout)
panels = list((home / ".cache/nacre/lock-ready").glob("dashboard-*-initial.png"))
assert panels and all(
    panel.read_bytes().startswith(b"\x89PNG\r\n\x1a\n") for panel in panels
)
print(
    "Installed native image/text renderer produces real lock panels without the developer SDK."
)
favorites = json.loads((home / ".config/nacre/launcher.json").read_text())["favorites"]
assert {
    "nacre-settings",
    "nacre-ai",
    "nacre-brain",
    "nacre-terminal",
    "org.kde.dolphin",
} <= set(favorites)
assert (
    "shell fish --init-command"
    in (home / ".config/nacre/terminal/kitty.conf").read_text()
)
run(
    "python",
    "-c",
    "import sys; sys.path.insert(0, '/usr/share/nacre/nacre/shell-tools'); from nacre_ai_controller import load; assert load()['default_assistant']() in ('codex','claude')",
)
print(
    "Real UWSM argument parsing, inherited keyboard syntax, default wallpaper/favorites/terminal and packaged AI controller imports pass."
)
for command in (
    ("nacre-welcome", "--help"),
    ("nacre-ai", "--help"),
    ("nacre-brain", "--help"),
    ("nacre-brain-maintain", "--help"),
    ("orient", "--version"),
    ("nacre_shell", "scheme", "list", "--names"),
):
    run(*command)
    located = Path(shutil.which(command[0]))
    installed = Path("/usr/bin") / command[0]
    assert located.samefile(installed), (command, str(located), str(installed))
assert account.read_text() == "fresh fixture, not credentials"
assert not (home / "Documents/Nacre-Brain").exists()
print(
    "Plan/apply, profile, keyboard, units, runtime links and installed CLI imports pass."
)
# Seeded state becomes user-owned, and repeat setup must leave it intact.
palette = home / ".local/state/nacre/scheme.json"
data = json.loads(palette.read_text())
data["userFixture"] = "preserve"
palette.write_text(json.dumps(data))
palette_hash = digest(palette)
run(*base, "--yes")
assert digest(keyboard) == keyboard_hash and digest(palette) == palette_hash
print("Repeated setup preserves keyboard and private palette.")
# Refuse drift before taking a new release snapshot.
managed = home / (
    ".config/hypr/conf/keyboard.lua"
    if backend == "hyprland"
    else ".config/nacre/niri/config.kdl"
)
original = managed.read_bytes()
managed.write_bytes(original + b"\n-- local fixture edit\n")
current = (home / ".local/state/nacre/releases/current.json").read_bytes()
refused = run(*base, "--yes", success=False)
assert "Local edit preserved" in refused.stdout + refused.stderr
assert (home / ".local/state/nacre/releases/current.json").read_bytes() == current
managed.write_bytes(original)
run("nacre", "rollback")
assert digest(palette) == palette_hash and account.exists()
run(*base, "--yes")
assert not (home / ".local/share/nacre/palette-runtime").is_symlink()
assert (home / ".local/share/nacre/shell/tools").resolve() == Path(
    "/usr/share/nacre/nacre/shell-tools"
)
print("Conflict refusal, package rollback and reapply pass without a desktop session.")
print("FRESH_USER_PACKAGE_PROOF_OK")
