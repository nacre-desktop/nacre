"""Warm medians of 31 runs; no desktop publication or compositor IPC."""

import argparse
import importlib.util
import json
import os
import sys
import tempfile
import time
import statistics
import hashlib
from pathlib import Path

root = Path(__file__).resolve().parents[1]

parser = argparse.ArgumentParser(
    description="Measure native RPC, Colors previews and an isolated palette apply."
)
parser.add_argument(
    "--gallery", type=Path, default=root / "deps/orient/tests/orient-gallery/images"
)
parser.add_argument("--output", type=Path, required=True)
args = parser.parse_args()
output, gallery = args.output, args.gallery
for key in (
    "NACRE_COMPOSITOR",
    "COMPOSITOR_TOOLS",
    "PEARL_TOOLS",
    "QS_CONFIG",
    "NIRI_SOCKET",
    "HYPRLAND_INSTANCE_SIGNATURE",
):
    os.environ.pop(key, None)
sys.path.insert(0, str(root / "nacre/shell-tools"))


def module(name):
    s = importlib.util.spec_from_file_location(
        name, root / "nacre/shell-tools" / f"{name}.py"
    )
    m = importlib.util.module_from_spec(s)
    s.loader.exec_module(m)
    return m


colors = module("colors")
classic = module("classic-state")
classic.compositor.query = lambda *a, **kw: []
from nacre_shell import _backend  # noqa: E402
from nacre_shell.engine import from_image  # noqa: E402
from nacre_shell.colour import lch  # noqa: E402

count = 0
original = _backend.call


def call(*args, **kw):
    global count
    count += 1
    return original(*args, **kw)


_backend.call = call
for m in tuple(sys.modules.values()):
    if (
        getattr(m, "__name__", "").startswith("nacre_shell.")
        and getattr(m, "call", None) is original
    ):
        m.call = call
results = {}


def measure(fn):
    global count
    fn()
    for m in tuple(sys.modules.values()):
        if (
            getattr(m, "__name__", "").startswith("nacre_shell.")
            and getattr(m, "call", None) is original
        ):
            m.call = call
    values = []
    counts = []
    hashes = []
    for _ in range(31):
        count = 0
        start = time.perf_counter_ns()
        result = fn()
        values.append((time.perf_counter_ns() - start) / 1e6)
        counts.append(count)
        hashes.append(
            hashlib.sha256(json.dumps(result, sort_keys=True).encode()).hexdigest()
        )
    assert len(set(hashes)) == 1
    return {
        "median_ms": statistics.median(values),
        "calls": counts[0],
        "result_sha256": hashes[0],
    }


def artifact_digest(path, home):
    content = path.read_bytes()
    if path.name == "scheme.json":
        # A version/source change legitimately invalidates the engine provenance.
        data = json.loads(content)
        data.pop("engine", None)
        content = json.dumps(data, sort_keys=True).encode()
    content = content.replace(str(home).encode(), b"/isolated-home").replace(
        str(root).encode(), b"/source"
    )
    return hashlib.sha256(content).hexdigest()


with tempfile.TemporaryDirectory() as folder:
    home = Path(folder)
    os.environ["XDG_CACHE_HOME"] = str(home / "cache")
    for name in ("coast-day", "warm-room", "aurora"):
        image = (
            root / "nacre/shell-tools/demo-wallpapers/aurora.jpg"
            if name == "aurora"
            else gallery / f"{name}.png"
        )
        data = from_image(image, cache_dir=home / "cache")
        results[name] = measure(lambda data=data: colors.image_colors(data))
    state = home / ".local/state/nacre"
    state.mkdir(parents=True)
    data = json.loads((root / "nacre/shell-tools/reference-style.json").read_text())
    (state / "scheme.json").write_text(json.dumps(data))

    def apply():
        classic.apply_palette(home, "/fixture/wallpaper.png", live=False)
        return {
            str(p.relative_to(home)): artifact_digest(p, home)
            for p in sorted(home.rglob("*"))
            if p.is_file()
            and p.name != "presentation.json"
            and p.name != "palette-commit.lock"
            and "cache" not in p.relative_to(home).parts
        }

    results["apply"] = measure(apply)
    results["lch"] = measure(lambda: [lch("aabbcc") for _ in range(100)])
output.write_text(json.dumps(results, indent=2) + "\n")
print(json.dumps(results, indent=2))
