#!/usr/bin/env bash
# Compatibility entry point for existing keybindings and private scripts.
set -euo pipefail
exec python3 "$(dirname "$(realpath "$0")")/compositor-compat.py" "$@"
