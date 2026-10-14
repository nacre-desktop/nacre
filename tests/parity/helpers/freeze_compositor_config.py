#!/usr/bin/env python3
"""Freeze current Python compiler outputs before the Wave 2 runtime port."""

import argparse
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[3]
TOOLS = ROOT / "nacre/shell-tools"
REFERENCE_SOURCES = {
    "compositor_config.py": "901c35e64dd2a44d9162609c381efa2670ed18feb4d67e7f77c514c8f8b75710",
    "compositor_backends/hyprland_config.py": "02d1fdb5ae8108cf979c268791d9a43641e34642b3e97a29072e7fa9d40920c4",
    "compositor_backends/niri_config.py": "af80f5f76d34481f75b6f5e6ec3ec9c632f78223acff1327f3bf20f925315020",
    "compositor_backends/hyprland.py": "e0c253121be18b66cbc6e149fefb00157ff6b6f6d427dde49135ef499d363859",
    "compositor_backends/niri.py": "b1befdc9049d11f53a3fdbf11eab038c5bd5f719f085c06371daecede4889847",
    "compositor-contract.json": "c075df8a6763f350a91f2910133cba1f5e00fdd8eeedc52e17dc40efade54c9b",
}
DESTINATION = (
    ROOT / "crates/nacre-helper/tests/fixtures/compositor-config-reference.json"
)


def verify_source():
    for name, digest in REFERENCE_SOURCES.items():
        source = TOOLS / name
        if (
            not source.is_file()
            or hashlib.sha256(source.read_bytes()).hexdigest() != digest
        ):
            raise ValueError(
                "Current Python compiler source is absent or changed: " + name
            )


def requests():
    def case(name, operation, *args, module="neutral", **kwargs):
        return {
            "name": name,
            "module": module,
            "operation": operation,
            "args": args,
            "kwargs": kwargs,
        }

    yield case("backends", "supported_backends")
    for backend in ("hyprland", "niri", "unknown"):
        yield case("capabilities-" + backend, "backend_capabilities", backend)
        yield case("render-" + backend, "render", "$contract", backend=backend)
        for values in (
            {},
            {"layout": "us", "options": "caps:escape"},
            {"unknown": "x"},
            {"layout": "us\nexec bad"},
        ):
            yield case(
                "keyboard-" + backend + "-" + str(values),
                "render_keyboard",
                values,
                backend=backend,
            )
    for index, value in enumerate(
        (
            None,
            True,
            False,
            0,
            -4,
            1.0,
            1.25,
            'quoted"\\\n\x01é',
            [],
            [1, "two", False],
            {"field": 1, "end": False, "not-a-field": "x"},
        )
    ):
        yield case("lua-value-" + str(index), "lua_value", value, module="hyprland")
        if isinstance(value, str):
            yield case("kdl-quoted-" + str(index), "quoted", value, module="niri")
    for index, target in enumerate(
        (
            {"kind": "number", "number": 2},
            {"kind": "number", "number": True},
            {"kind": "number", "number": 0},
            {"kind": "scratchpad", "name": ""},
            {"kind": "scratchpad", "name": "scratch"},
            {"kind": "relative", "scope": "monitor", "offset": -2},
            {"kind": "relative", "scope": "existing", "offset": 0},
            {"kind": "relative", "scope": "global", "offset": 3},
            {"kind": "relative", "scope": "unknown", "offset": 1},
            {"kind": "number", "number": 1, "extra": True},
        )
    ):
        yield case("workspace-" + str(index), "workspace_target", target)
    profiles = (
        {"routes": [], "brainWorkspace": None},
        {
            "routes": [{"name": "editor", "match": "^Example$", "workspace": 2}],
            "brainWorkspace": 3,
        },
        {"routes": [], "brainWorkspace": None, "windowTrashBindings": True},
    )
    for index, profile in enumerate(profiles):
        yield case("profile-rules-" + str(index), "profile_rules", profile)
        for backend in ("hyprland", "niri", "unknown"):
            yield case(
                "profile-" + str(index) + "-" + backend,
                "render_profile",
                profile,
                backend=backend,
            )
        value = case(
            "profile-named-" + str(index), "render_profile", profile, backend="niri"
        )
        # Python requires integer keys. Keep their type without changing the
        # production API or silently turning JSON string keys into integers.
        value["workspace_names"] = [[2, "work"], [3, "notes"]]
        yield value
    for mutation in (
        "unknown-root",
        "bad-version",
        "unknown-action",
        "unknown-output",
        "bad-selector",
        "bad-float",
    ):
        value = case("invalid-" + mutation, "render", "$contract")
        value["mutation"] = mutation
        yield value


def child(request):
    verify_source()
    sys.path.insert(0, str(TOOLS))
    spec = importlib.util.spec_from_file_location(
        "reference_config", TOOLS / "compositor_config.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    modules = {"neutral": module, "hyprland": module.hyprland, "niri": module.niri}
    contract = module.load()
    mutation = request.get("mutation")
    if mutation == "unknown-root":
        contract["unknown"] = True
    elif mutation == "bad-version":
        contract["version"] = 0
    elif mutation == "unknown-action":
        contract["outputs"]["hypr/conf/keybinding.lua"][0]["action"] = "unknown"
    elif mutation == "unknown-output":
        contract["outputs"]["unknown.lua"] = []
    elif mutation == "bad-selector":
        contract["outputs"]["hypr/conf/windowrule.lua"][0]["selector"][
            "app_id_regex"
        ] = True
    elif mutation == "bad-float":
        contract["outputs"]["hypr/conf/windowrule.lua"][0]["effects"]["floating"] = (
            "false"
        )
    args = [
        copy.deepcopy(contract) if value == "$contract" else value
        for value in request["args"]
    ]
    kwargs = request["kwargs"]
    if "workspace_names" in request:
        kwargs["workspace_names"] = dict(request["workspace_names"])
    try:
        result = getattr(modules[request["module"]], request["operation"])(
            *args, **kwargs
        )
        print(json.dumps(result, allow_nan=False))
    except (ValueError, RuntimeError, OSError, KeyError, TypeError) as error:
        print(json.dumps({"error": str(error), "type": type(error).__name__}))
        return 1
    return 0


def capture(destination):
    verify_source()
    if destination.exists():
        raise ValueError("Refusing to overwrite frozen compiler outputs")
    cases = []
    for request in requests():
        with tempfile.TemporaryDirectory(prefix="compiler-reference-") as directory:
            home = Path(directory)
            environment = {
                "HOME": directory,
                "PATH": "",
                "PYTHONDONTWRITEBYTECODE": "1",
                "XDG_CONFIG_HOME": str(home / ".config"),
                "XDG_STATE_HOME": str(home / ".local/state"),
                "XDG_CACHE_HOME": str(home / ".cache"),
                "NACRE_LANGUAGE": "en",
            }
            result = subprocess.run(
                [
                    sys.executable,
                    str(Path(__file__).resolve()),
                    "--child",
                    json.dumps(request),
                ],
                env=environment,
                stdin=subprocess.DEVNULL,
                capture_output=True,
                text=True,
                timeout=10,
                check=False,
            )
            files = {
                str(path.relative_to(home)): {
                    "hex": path.read_bytes().hex(),
                    "mode": oct(path.stat().st_mode & 0o777),
                }
                for path in sorted(home.rglob("*"))
                if path.is_file()
            }
            # All operations here are pure: an unexpected traceback/write is
            # a capture failure, never a new expected result.
            if result.stderr or files or result.returncode not in (0, 1):
                raise ValueError(
                    "Unexpected compiler capture effect: " + request["name"]
                )
            cases.append(
                {
                    "request": request,
                    "stdout": result.stdout,
                    "stderr": result.stderr,
                    "exit": result.returncode,
                    "files": files,
                }
            )
    verify_source()
    destination.parent.mkdir(parents=True, exist_ok=True)
    with destination.open("x") as output:
        output.write(
            json.dumps(
                {
                    "reference_sources": REFERENCE_SOURCES,
                    "contract": json.loads(
                        (TOOLS / "compositor-contract.json").read_text()
                    ),
                    "cases": cases,
                },
                indent=2,
            )
            + "\n"
        )
    return len(cases)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=DESTINATION)
    parser.add_argument("--child")
    args = parser.parse_args()
    if args.child:
        return child(json.loads(args.child))
    print("Frozen compiler cases:", capture(args.output))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
