"""Exercise compiler-free setup from the signed repository, with Niri absent."""

import json
import os
from pathlib import Path
import shutil
import subprocess

home = Path.home()
assert os.geteuid() != 0 and home == Path("/home/nacre-package-test-delivery")
assert not (home / ".config/nacre").exists()
assert shutil.which("niri") is None and shutil.which("hyprland") is None
os.chdir(home)  # Never fall back to the CI source checkout for helper assets.
plain = home / ".config/niri/config.kdl"
plain.parent.mkdir(parents=True)
plain.write_text("// Existing distribution session fixture\n")
plain_bytes = plain.read_bytes()
command = [
    "nacre",
    "setup",
    "--non-interactive",
    "--profile",
    "default",
    "--compositor",
    "niri",
    "--layout",
    "no",
]
subprocess.run([*command, "--plan"], check=True)
assert shutil.which("niri") is None
assert not (home / ".config/nacre").exists()
subprocess.run([*command, "--yes", "--install-deps"], check=True)
assert Path(shutil.which("niri")).samefile("/usr/bin/niri")
assert plain.read_bytes() == plain_bytes
assert (home / ".config/nacre/niri/config.kdl").is_file()
assert (home / ".config/systemd/user/nacre-niri.target").is_file()
state = json.loads(subprocess.check_output(["nacre", "status"], text=True))
assert state["status"] == "prepared"
assert (
    subprocess.check_output(["pacman", "-Qoq", "/usr/bin/orient"], text=True).strip()
    == "orient"
)
assert (
    subprocess.check_output(
        ["pacman", "-Qoq", "/usr/lib/nacre/nacre-helper"], text=True
    ).strip()
    == "nacre"
)
version = subprocess.check_output(["pacman", "-Q", "niri"], text=True)
subprocess.run([*command, "--yes"], check=True)
assert subprocess.check_output(["pacman", "-Q", "niri"], text=True) == version
assert plain.read_bytes() == plain_bytes
print("SIGNED_REPOSITORY_MISSING_NIRI_SETUP_OK")
