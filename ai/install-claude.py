#!/usr/bin/env python3
"""Install the isolated Claude session-history SDK; CLI/auth remain separate."""

from pathlib import Path
import subprocess
import sys

root = Path.home() / ".local/share/nacre/ai/claude-sdk"
subprocess.run([sys.executable, "-m", "venv", str(root)], check=True)
subprocess.run(
    [str(root / "bin/python"), "-m", "pip", "install", "claude-agent-sdk==0.2.162"],
    check=True,
)
print(
    "Claude history support installed. Install Claude Code from https://code.claude.com/docs/en/setup and run claude auth login."
)
