"""Installed-package acceptance; run only as a fresh nacre-package-test* user.

This deliberately refuses existing configuration/account directories. Use a
VM/disposable runtime root, never a maintainer's account.
"""

import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess

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
base = (
    "nacre",
    "setup",
    "--non-interactive",
    "--profile",
    "default",
    "--compositor",
    "hyprland",
    "--layout",
    "no",
    "--with",
    "ai",
    "--with",
    "brain",
)
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
keyboard_hash = digest(keyboard)
for relative in (
    ".config/quickshell/nacre",
    ".local/share/nacre/shell/tools",
    ".local/share/nacre/brain",
    ".local/share/nacre/ai/application",
    ".config/systemd/user/graphical-session.target.wants/nacre-shell.service",
    ".config/systemd/user/timers.target.wants/nacre-brain-check.timer",
):
    path = home / relative
    assert path.is_symlink() and path.exists(), relative
for command in (
    ("nacre-welcome", "--help"),
    ("nacre-ai", "--help"),
    ("nacre-brain", "--help"),
    ("nacre-brain-maintain", "--help"),
    ("orient", "--version"),
    ("nacre_shell", "scheme", "list", "--names"),
):
    run(*command)
    assert shutil.which(command[0]).startswith("/usr/bin/"), command
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
managed = home / ".config/hypr/conf/keyboard.lua"
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
