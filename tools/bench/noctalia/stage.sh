#!/usr/bin/env bash
# Build the pinned Noctalia and the Nacre checkout at /workspace into one stage dir.
# Runs as root in the prebuilt nacre-ci/arch-nacre image (host.sh stage); writes only STAGE.
#   stage.sh STAGE NACRE_COMMIT [UID:GID]
set -euo pipefail
stage=${1:?stage dir}
nacre_commit=${2:?full Nacre commit}
owner=${3:-}
src=/workspace
: "${NOCTALIA_TAG:=v5.4.1}"
export CARGO_TARGET_DIR=/build/target
# Keep symbols for perf attribution; .symtab is not loaded, so memory and code are unchanged.
export CARGO_PROFILE_RELEASE_STRIP=false
head=$(git -c safe.directory='*' -C "$src" rev-parse HEAD)
[[ "$head" == "$nacre_commit" ]] || { echo "/workspace is $head, not $nacre_commit" >&2; exit 1; }
# Not in the image (reported to the Release lead): Noctalia's build tools and libraries, wlr-randr.
nice -n 10 pacman -Syu --noconfirm --needed meson ninja wayland-protocols nlohmann-json stb \
  libsecret libsodium sdbus-cpp wireplumber polkit pam libwebp libjxl libsndfile libqalculate \
  libxml2 md4c tomlplusplus libical jemalloc wlr-randr
rm -rf "$stage"
mkdir -p "$stage/bin" "$stage/fonts" "$stage/nacre/nacre" "$stage/nacre/config/niri" /build

git clone --branch "$NOCTALIA_TAG" --depth 1 https://github.com/noctalia-dev/noctalia.git /build/noctalia
nice -n 10 meson setup /build/noctalia/build /build/noctalia --buildtype=release \
  --prefix="$stage/noctalia" -Dtests=disabled -Djemalloc=enabled
nice -n 10 meson compile -C /build/noctalia/build -j2
meson install -C /build/noctalia/build

cd "$src"
nice -n 10 cargo build --release --locked -j2 -p nacre-shell --bin nacre-shell \
  --bin bead-wallpaper-video --bin nacre-wallpaper-decode
nice -n 10 cargo build --release --locked -j2 -p nacre-helper --bin nacre-helper
for name in nacre-shell bead-wallpaper-video nacre-wallpaper-decode nacre-helper; do
  install -m755 "$CARGO_TARGET_DIR/release/$name" "$stage/bin/$name"
done
# What bench.py reads from the source tree: Luau shell, helper tools, niri rules.
cp -a shell "$stage/nacre/shell"
cp -a nacre/shell-tools "$stage/nacre/nacre/shell-tools"
cp config/niri/window-rules.kdl "$stage/nacre/config/niri/"
# compositor_boundary.py: the staged stock compositor is only ever a copied file here.
install -m755 /usr/bin/niri "$stage/bin/stock-niri"
install -m755 /usr/bin/wlr-randr "$stage/bin/wlr-randr"

python - "$stage" <<'PY'
import hashlib, json, sys, urllib.request
from pathlib import Path
stage = Path(sys.argv[1])
manifest = json.loads((stage / "nacre/nacre/shell-tools/font-assets.json").read_text())
for group in manifest["groups"]:
    if not group.get("default"):
        continue
    for item in group["files"]:
        target = stage / "fonts" / item["name"]
        with urllib.request.urlopen(item["url"], timeout=120) as response:
            target.write_bytes(response.read())
        if hashlib.sha256(target.read_bytes()).hexdigest() != item["sha256"]:
            raise SystemExit("font hash mismatch: " + item["name"])
PY

python - "$stage" "$nacre_commit" "$NOCTALIA_TAG" <<'PY'
import hashlib, json, re, subprocess, sys
from pathlib import Path
stage, commit, tag = Path(sys.argv[1]), sys.argv[2], sys.argv[3]
def sha(path): return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def out(*cmd): return subprocess.run(cmd, capture_output=True, text=True, check=True).stdout.strip()
manifest = Path("/workspace/crates/nacre-shell/Cargo.toml").read_text()
pins = {
    "nacre": commit,
    "bead": sorted(set(re.findall(r'nacre-desktop/bead", rev = "([0-9a-f]+)"', manifest))),
    "nacre_binaries": "release, strip=false",
    "noctalia_tag": tag,
    "noctalia": out("git", "-C", "/build/noctalia", "rev-parse", "HEAD"),
    "stock_niri": out("pacman", "-Q", "niri"),
    "rustc": out("rustc", "--version"),
    "mesa": out("pacman", "-Q", "mesa"),
    "sha256": {name: sha(stage / "bin" / name) for name in ("nacre-shell", "nacre-helper", "bead-wallpaper-video", "nacre-wallpaper-decode", "stock-niri")},
}
pins["sha256"]["noctalia"] = sha(stage / "noctalia/bin/noctalia")
(stage / "pins.json").write_text(json.dumps(pins, indent=1) + "\n")
print(json.dumps(pins, indent=1))
PY
if [[ -n "$owner" ]]; then chown -R "$owner" "$stage"; fi
echo "STAGE OK $stage"
