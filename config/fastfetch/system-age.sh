#!/usr/bin/env bash
# Filesystem creation age, not an asserted operating-system installation date.
set -euo pipefail
birth=$(stat --format=%W / 2>/dev/null) || birth=0
now=$(date +%s)
if [[ $birth =~ ^[0-9]+$ && $now =~ ^[0-9]+$ ]] && (( birth > 0 && birth <= now )); then
    printf '%s days\n' "$(( (now - birth) / 86400 ))"
else
    printf 'Unknown\n'
fi
