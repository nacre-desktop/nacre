#!/usr/bin/env bash
# Source/package build only. Installed desktops use the resulting native binary.
set -euo pipefail
repo_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
if [[ $# != 0 ]]; then
    printf 'Usage: tools/build-helper.sh\n' >&2
    exit 2
fi
if ! command -v cargo >/dev/null; then
    printf 'Building Nacre from source requires Rust and Cargo build tools.\n' >&2
    exit 1
fi
cargo build --locked --release --manifest-path "$repo_dir/Cargo.toml" \
    --target-dir "$repo_dir/target" -p nacre-helper --bin nacre-helper >&2
printf '%s\n' "$repo_dir/target/release/nacre-helper"
