"""Installed-package acceptance; run only as a fresh nacre-package-test* user.

This deliberately refuses existing configuration/account directories. Use a
VM/disposable runtime root, never a maintainer's account.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

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
assert run("pacman", "-Qoq", "/usr/bin/orient").stdout.strip() == "orient"
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
if backend in {"hyprland", "both"}:
    assert Path("/usr/share/wayland-sessions/nacre-hyprland.desktop").is_file()
    assert (home / ".config/systemd/user/nacre-hyprland.target").is_file()
wallpaper = Path((home / ".local/state/nacre/wallpaper/last.txt").read_text().strip())
assert wallpaper.is_file() and wallpaper.suffix == ".jpg"
presentation = json.loads((home / ".local/state/nacre/presentation.json").read_text())
assert presentation["poster"] == str(wallpaper) and presentation["colours"]
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
assert (home / ".local/share/nacre/palette-runtime").resolve() == Path(
    "/usr/share/nacre/runtime/palette"
)
print("Conflict refusal, package rollback and reapply pass without a desktop session.")
print("FRESH_USER_PACKAGE_PROOF_OK")
