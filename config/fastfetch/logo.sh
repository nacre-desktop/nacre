#!/usr/bin/env bash
# Text fallback is rasterized from the same shell geometry as the graphical logo;
# truecolour terminals get each braille row in its live palette colour.
logo_file="$HOME/.local/share/nacre/branding/nacre-text.txt"
colour_file="${logo_file%.txt}.ans"
if [[ -z "${NO_COLOR:-}" && "${COLORTERM:-}" =~ ^(truecolor|24bit)$ && -f "$colour_file" ]]; then
    cat "$colour_file"
elif [[ -f "$logo_file" ]]; then
    printf '\033[38;5;4m'
    cat "$logo_file"
    printf '\033[0m\n'
else
    printf 'Nacre\n'
fi
