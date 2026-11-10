#!/usr/bin/env bash

cache_dir="${XDG_CACHE_HOME:-$HOME/.cache}/fastfetch"
cache_file="$cache_dir/nacre-logo.txt"

mkdir -p "$cache_dir"
"$HOME/.local/bin/nacre-terminal-logo" --prepare
"$HOME/.config/fastfetch/logo.sh" >"$cache_file"
