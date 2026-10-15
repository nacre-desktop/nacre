"""Replacement routing for the completed native helper cutover.

Historical source paths and golden files remain reference evidence. A missing
native build is an error, never permission to run the retired Python engine.
"""

import os
from pathlib import Path
import shlex

ROOT = Path(__file__).resolve().parents[3]
MIGRATED = frozenset(
    {
        "dashboard-data",
        "dashboard-media",
        "dashboard-system",
        "diagnostics",
        "helper-service",
        "kde-palette",
        "terminal-branding",
        "login-appearance",
        "demo-wallpapers",
        "wallpaper-watch",
        "wallpaper-media",
        "colors",
        "classic-state",
        "branding",
        "file-icons",
        "lock-config",
        "lock-dashboard",
        "palette_engine",
        "nacre-colors",
        "nacre_shell",
        "launcher-preferences",
        "light-devices",
        "menu-backend",
        "menu-cli",
        "network-state",
        "notification-history",
        "pearl-state",
        "pearl-window",
        "compositor",
        "compositor-compat",
        "compositor-backends-hyprland",
        "compositor-backends-niri",
        "resource-state",
        "window-chat-title",
        "window-hide",
        "compositor_config",
        "compositor-backends-hyprland-config",
        "compositor-backends-niri-config",
        "compositor-backends-niri-widths",
    }
)


def native_binary(root=ROOT, environ=None, *, module_api=False):
    """Resolve an explicit build or this checkout's installed/compiled helper."""
    environ = os.environ if environ is None else environ
    override = environ.get("NACRE_PARITY_API" if module_api else "NACRE_HELPER_BINARY")
    candidates = (
        [Path(override)]
        if override
        else [
            root / "target/release/examples/parity-api",
            root / "target/debug/examples/parity-api",
        ]
        if module_api
        else [
            root / "nacre/shell-tools/nacre-helper",
            root / "target/release/nacre-helper",
            root / "target/debug/nacre-helper",
        ]
    )
    executable = [
        path for path in candidates if path.is_file() and os.access(path, os.X_OK)
    ]
    if executable:
        if override or (not module_api and executable[0] == candidates[0]):
            return executable[0].resolve()
        return max(executable, key=lambda path: path.stat().st_mtime_ns).resolve()
    raise ValueError(
        "Native helper binary is unavailable; build nacre-helper or set "
        + ("NACRE_PARITY_API" if module_api else "NACRE_HELPER_BINARY")
        + " to the executable under test"
    )


def command(case, override=None, *, root=ROOT, environ=None):
    """Return an argv template, or None for an unchanged reference helper."""
    if override is not None:
        return override
    helper = case["helper"]
    original = shlex.split(case.get("command", ""))
    if helper == "provision":
        # Historical developer-only provisioning checks; the runtime is retired.
        return shlex.join(
            [
                "python3",
                "tests/parity/helpers/api.py",
                "nacre/shell-tools/tests/reference_palette_provision.py",
            ]
        )
    if helper not in MIGRATED:
        return None
    if helper == "nacre_shell" and original:
        if original == ["python3", "-m", "nacre_shell"]:
            binary = native_binary(root, environ)
            environment = os.environ if environ is None else environ
            override = environment.get("NACRE_PALETTE_CLI_PARITY")
            candidates = (
                [Path(override)]
                if override
                else [
                    binary.parent / "examples/palette-cli-parity",
                    root / "target/debug/examples/palette-cli-parity",
                    root / "target/release/examples/palette-cli-parity",
                ]
            )
            available = [
                path
                for path in candidates
                if path.is_file() and os.access(path, os.X_OK)
            ]
            if not available:
                raise ValueError("Build the developer palette-cli-parity provider")
            provider = (
                available[0]
                if override
                else max(available, key=lambda path: path.stat().st_mtime_ns)
            )
            return shlex.join([str(provider.resolve()), "python3 -m nacre_shell"])
        if original == ["python3", "tests/parity/helpers/api.py", "nacre_shell.engine"]:
            return shlex.join(
                [str(native_binary(root, environ, module_api=True)), "nacre_shell"]
            )
        raise ValueError("Unsupported native palette CLI contract command")
    if helper == "nacre-colors" and original == ["python3", "bin/nacre-colors"]:
        return shlex.join([str(native_binary(root, environ)), helper])

    if (
        helper
        in {"compositor", "compositor-backends-hyprland", "compositor-backends-niri"}
        and "args" in case
    ):
        environment = os.environ if environ is None else environ
        override = environment.get("NACRE_COMPOSITOR_REFERENCE")
        candidates = (
            [Path(override)]
            if override
            else [
                root / "target/debug/examples/compositor-reference",
                root / "target/release/examples/compositor-reference",
            ]
        )
        available = [
            path for path in candidates if path.is_file() and os.access(path, os.X_OK)
        ]
        if not available:
            raise ValueError("Build the developer compositor-reference provider")
        args = [str(available[0].resolve())]
        if helper != "compositor":
            args.extend(["module", helper.removeprefix("compositor-backends-")])
        return shlex.join(args)
    if helper == "pearl-window" and "commands" in case:
        environment = os.environ if environ is None else environ
        override = environment.get("NACRE_PEARL_WINDOW_REFERENCE")
        candidates = (
            [Path(override)]
            if override
            else [
                root / "target/debug/examples/pearl-window-reference",
                root / "target/release/examples/pearl-window-reference",
            ]
        )
        available = [
            path for path in candidates if path.is_file() and os.access(path, os.X_OK)
        ]
        if not available:
            raise ValueError("Build the developer pearl-window-reference adapter")
        return shlex.join([str(available[0].resolve())])
    if helper == "compositor-backends-niri-widths":
        module_api = len(original) > 1 and original[1] == "tests/parity/helpers/api.py"
        expected = ["python3"]
        if module_api:
            expected.append("tests/parity/helpers/api.py")
        expected.append("nacre/shell-tools/compositor_backends/niri_widths.py")
        if original and original != expected:
            raise ValueError("Unsupported native width observer contract command")
        return shlex.join(
            [
                str(native_binary(root, environ, module_api=module_api)),
                helper if module_api else "niri_widths",
            ]
        )
    compiler_sources = {
        "compositor_config": "compositor_config.py",
        "compositor-backends-hyprland-config": "compositor_backends/hyprland_config.py",
        "compositor-backends-niri-config": "compositor_backends/niri_config.py",
    }
    if helper in compiler_sources:
        expected = [
            "python3",
            "tests/parity/helpers/api.py",
            "nacre/shell-tools/" + compiler_sources[helper],
        ]
        if original and original != expected:
            raise ValueError("Unsupported native compiler contract command")
        return shlex.join([str(native_binary(root, environ)), helper])
    if helper == "diagnostics" and original == [
        "python3",
        "tests/parity/helpers/diagnostics_reference.py",
    ]:
        environment = os.environ if environ is None else environ
        override = environment.get("NACRE_DIAGNOSTICS_PARITY")
        candidates = (
            [Path(override)]
            if override
            else [
                root / "target/release/examples/diagnostics-parity",
                root / "target/debug/examples/diagnostics-parity",
            ]
        )
        available = [
            path for path in candidates if path.is_file() and os.access(path, os.X_OK)
        ]
        if not available:
            raise ValueError("Build the developer diagnostics-parity example")
        binary = (
            available[0]
            if override
            else max(available, key=lambda p: p.stat().st_mtime_ns)
        )
        return shlex.join([str(binary.resolve())])
    module_api = len(original) > 1 and original[1] == "tests/parity/helpers/api.py"
    if module_api and original != [
        "python3",
        "tests/parity/helpers/api.py",
        f"nacre/shell-tools/{helper}.py",
    ]:
        raise ValueError("Unsupported native module contract command")
    binary = native_binary(root, environ, module_api=module_api)
    operation = "serve" if helper == "helper-service" else helper
    return shlex.join([str(binary), operation])


def assert_recordable(cases):
    """Check the entire selection before any reference file can be modified."""
    migrated = sorted({case["helper"] for case in cases} & (MIGRATED | {"provision"}))
    if migrated:
        raise ValueError(
            "Migrated helper references cannot be re-recorded: " + ", ".join(migrated)
        )


def service_fixture_binary(root=ROOT, environ=None):
    """Resolve the developer provider needed by the frozen service dialogues."""
    environ = os.environ if environ is None else environ
    override = environ.get("NACRE_SERVICE_FIXTURE")
    candidates = (
        [Path(override)]
        if override
        else [
            root / "target/release/examples/service-fixture",
            root / "target/debug/examples/service-fixture",
        ]
    )
    executable = [
        path for path in candidates if path.is_file() and os.access(path, os.X_OK)
    ]
    if executable:
        return max(executable, key=lambda path: path.stat().st_mtime_ns).resolve()
    raise ValueError("Build service-fixture or set NACRE_SERVICE_FIXTURE")
