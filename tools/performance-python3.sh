#!/usr/bin/env bash
# Put a symlink named python3 to this script first in an isolated shell's PATH.
# Monotonic boot seconds and PID only: never record private helper arguments.
set -euo pipefail
read -r performance_uptime performance_unused < /proc/uptime
printf '%s %s\n' "$performance_uptime" "$$" >> "${NACRE_PERF_SPAWN_LOG:?}"
exec "${NACRE_PERF_PYTHON:?}" "$@"
