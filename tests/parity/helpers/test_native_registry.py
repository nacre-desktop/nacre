"""Default routing must neither fall back nor overwrite pre-port references."""

import importlib.util
import os
from pathlib import Path
import shlex
import tempfile
import unittest
import subprocess
import sys

HERE = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location(
    "native_registry", HERE / "native_registry.py"
)
registry = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(registry)
API_SPEC = importlib.util.spec_from_file_location("helper_module_api", HERE / "api.py")
api = importlib.util.module_from_spec(API_SPEC)
API_SPEC.loader.exec_module(api)


class NativeRegistryTests(unittest.TestCase):
    def test_compiler_module_commands_are_exact_and_cannot_fall_back(self):
        with tempfile.TemporaryDirectory() as directory:
            binary = self.executable(Path(directory) / "helper")
            for helper, source in (
                ("compositor_config", "compositor_config.py"),
                (
                    "compositor-backends-hyprland-config",
                    "compositor_backends/hyprland_config.py",
                ),
                (
                    "compositor-backends-niri-config",
                    "compositor_backends/niri_config.py",
                ),
            ):
                case = {
                    "helper": helper,
                    "command": "python3 tests/parity/helpers/api.py nacre/shell-tools/"
                    + source,
                }
                environment = {"NACRE_HELPER_BINARY": str(binary)}
                self.assertEqual(
                    shlex.split(registry.command(case, environ=environment)),
                    [str(binary), helper],
                )
                case["command"] += " unexpected"
                with self.assertRaisesRegex(ValueError, "Unsupported native compiler"):
                    registry.command(case, environ=environment)

    def test_release_helper_uses_separately_built_debug_cli_provider(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = self.executable(root / "target/release/nacre-helper")
            provider = self.executable(
                root / "target/debug/examples/palette-cli-parity"
            )
            case = {"helper": "nacre_shell", "command": "python3 -m nacre_shell"}
            environment = {"NACRE_HELPER_BINARY": str(binary)}
            self.assertEqual(
                shlex.split(registry.command(case, root=root, environ=environment)),
                [str(provider.resolve()), "python3 -m nacre_shell"],
            )
            environment["NACRE_PALETTE_CLI_PARITY"] = str(
                root / "missing-explicit-provider"
            )
            with self.assertRaisesRegex(ValueError, "Build the developer palette-cli"):
                registry.command(case, root=root, environ=environment)

    def executable(self, path):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text("#!/bin/sh\nexit 0\n")
        path.chmod(0o700)
        return path

    def test_all_migrated_helpers_route_to_native_and_service_uses_serve(self):
        with tempfile.TemporaryDirectory(prefix="native registry ") as directory:
            binary = self.executable(Path(directory) / "helper binary")
            for helper in registry.MIGRATED:
                with self.subTest(helper=helper):
                    argv = shlex.split(
                        registry.command(
                            {"helper": helper},
                            environ={"NACRE_HELPER_BINARY": str(binary)},
                        )
                    )

    def test_weather_and_updates_module_cases_use_standalone_native_commands(self):
        with tempfile.TemporaryDirectory() as directory:
            binary = self.executable(Path(directory) / "helper")
            for helper in ("weather", "updates"):
                case = {
                    "helper": helper,
                    "command": f"python3 tests/parity/helpers/api.py nacre/shell-tools/{helper}.py",
                }
                self.assertEqual(
                    shlex.split(
                        registry.command(
                            case, environ={"NACRE_HELPER_BINARY": str(binary)}
                        )
                    ),
                    [str(binary.resolve()), helper],
                )
                    self.assertEqual(argv[0], str(binary.resolve()))
                    self.assertEqual(
                        argv[1],
                        {
                            "helper-service": "serve",
                            "compositor-backends-niri-widths": "niri_widths",
                        }.get(helper, helper),
                    )

    def test_missing_or_nonexecutable_native_build_never_falls_back(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "nacre/shell-tools/menu-backend.py"
            source.parent.mkdir(parents=True)
            source.touch()
            for environ in ({}, {"NACRE_HELPER_BINARY": str(source)}):
                with self.assertRaisesRegex(ValueError, "unavailable"):
                    registry.command(
                        {"helper": "menu-backend"}, root=root, environ=environ
                    )

    def test_explicit_binary_is_authoritative_and_installed_precedes_builds(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            installed = self.executable(root / "nacre/shell-tools/nacre-helper")
            self.executable(root / "target/debug/nacre-helper")
            self.assertEqual(registry.native_binary(root, {}), installed)
            with self.assertRaisesRegex(ValueError, "unavailable"):
                registry.native_binary(
                    root, {"NACRE_HELPER_BINARY": str(root / "missing")}
                )

    def test_newest_local_build_is_selected_over_stale_release(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            release = self.executable(root / "target/release/nacre-helper")
            debug = self.executable(root / "target/debug/nacre-helper")
            os.utime(release, ns=(10, 10))
            os.utime(debug, ns=(20, 20))
            self.assertEqual(registry.native_binary(root, {}), debug)
            os.utime(release, ns=(30, 30))
            self.assertEqual(registry.native_binary(root, {}), release)

    def test_untouched_helpers_and_explicit_overrides_keep_their_boundary(self):
        self.assertIsNone(registry.command({"helper": "nacre-app"}, environ={}))
        self.assertEqual(
            registry.command({"helper": "menu-backend"}, "candidate {name}"),
            "candidate {name}",
        )

    def test_recording_mixed_selection_is_rejected_before_reference_execution(self):
        registry.assert_recordable([{"helper": "weather"}])
        for helper in registry.MIGRATED:
            with self.subTest(helper=helper):
                with self.assertRaisesRegex(ValueError, "cannot be re-recorded"):
                    registry.assert_recordable(
                        [{"helper": "weather"}, {"helper": helper}]
                    )

    def test_module_boundary_uses_only_the_developer_adapter(self):
        case = {
            "helper": "dashboard-data",
            "command": "python3 tests/parity/helpers/api.py nacre/shell-tools/dashboard-data.py",
        }
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.executable(root / "target/debug/nacre-helper")
            with self.assertRaisesRegex(ValueError, "NACRE_PARITY_API"):
                registry.command(case, root=root, environ={})
            adapter = self.executable(root / "target/debug/examples/parity-api")
            self.assertEqual(
                shlex.split(registry.command(case, root=root, environ={})),
                [str(adapter), "dashboard-data"],
            )
            case["command"] += " unexpected-argument"
            with self.assertRaisesRegex(ValueError, "Unsupported"):
                registry.command(case, root=root, environ={})

    def test_cli_rejects_recording_before_touching_any_migrated_reference(self):
        references = sorted(HERE.glob("launcher-preferences/expected-*.json"))
        before = {path: path.read_bytes() for path in references}
        output = subprocess.run(
            [
                sys.executable,
                str(HERE / "run.py"),
                "--record",
                "--case",
                "launcher-preferences/*",
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(output.returncode, 2)
        self.assertIn("cannot be re-recorded", output.stderr)
        self.assertEqual({path: path.read_bytes() for path in references}, before)

    def test_service_fixture_override_cannot_fall_back_to_another_build(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = self.executable(root / "target/debug/examples/service-fixture")
            self.assertEqual(registry.service_fixture_binary(root, {}), binary)
            with self.assertRaisesRegex(ValueError, "NACRE_SERVICE_FIXTURE"):
                registry.service_fixture_binary(
                    root, {"NACRE_SERVICE_FIXTURE": str(root / "missing")}
                )

    def test_reference_datetime_tag_is_explicit_and_preserves_timezone(self):
        decoded = api.decode({"args": [{"$datetime": "2099-10-11T00:00:00+02:00"}]})
        self.assertEqual(decoded["args"][0].isoformat(), "2099-10-11T00:00:00+02:00")
        ordinary = {"$datetime": "not parsed", "other": True}
        self.assertEqual(api.decode(ordinary), ordinary)
        with self.assertRaises(ValueError):
            api.decode({"$datetime": "invalid"})


if __name__ == "__main__":
    unittest.main()
