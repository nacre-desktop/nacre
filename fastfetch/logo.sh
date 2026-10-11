#!/usr/bin/env bash
# Text fallback is rasterized from the same shell geometry as the graphical logo.
logo_file="$HOME/.local/share/nacre/branding/nacre-text.txt"
if [[ -f "$logo_file" ]]; then
    printf '\033[38;5;4m'
    cat "$logo_file"
    printf '\033[0m\n'
else
    printf 'Nacre\n'
fi
