"""Neutral declarations preserve unchanged behavior and the current shortcut contract."""

import copy
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
DIRECTORY = ROOT / "nacre/shell-tools"
sys.path.insert(0, str(ROOT / "tools"))
from bindings import capture

spec = importlib.util.spec_from_file_location(
    "compositor_config", DIRECTORY / "compositor_config.py"
)
config = importlib.util.module_from_spec(spec)
spec.loader.exec_module(config)


class CompositorConfigTests(unittest.TestCase):
    def test_all_checked_in_output_matches_authoritative_source(self):
        for path, output in config.render(config.load()).items():
            with self.subTest(path=path):
                self.assertEqual((ROOT / path).read_text(), output)

    @unittest.skipUnless(shutil.which("lua"), "Lua unavailable")
    def test_registrations_match_defaults_and_unchanged_historical_contracts(self):
        baseline = json.loads(
            (ROOT / "tools/tests/fixtures/compositor-config-contract.json").read_text()
        )
        self.assertEqual(set(baseline), set(config.OUTPUTS))
        defaults = json.loads(
            (ROOT / "tools/tests/fixtures/keybindings-defaults.json").read_text()
        )
        for path, expected in baseline.items():
            with self.subTest(path=path):
                actual = capture(ROOT / path, "declarations")
                if path in defaults:
                    self.assertEqual(
                        [row["value"] for row in actual if row["kind"] == "binding"],
                        defaults[path],
                    )
                    actual = [row for row in actual if row["kind"] != "binding"]
                    expected = [row for row in expected if row["kind"] != "binding"]
                self.assertEqual(actual, expected)

    @unittest.skipUnless(shutil.which("lua"), "Lua unavailable")
    def test_editing_neutral_source_changes_binding_and_window_rule(self):
        contract = config.load()
        bindings = contract["outputs"]["hypr/conf/keybinding.lua"]
        switch = next(row for row in bindings if row.get("keys") == "SUPER + 1")
        switch["keys"] = "SUPER + 8"
        switch["parameters"]["workspace"]["number"] = 8
        rules = contract["outputs"]["hypr/conf/windowrule.lua"]
        rules[0]["selector"]["app_id_regex"] = "^Example$"
        rules[0]["effects"]["floating"] = False
        rules[0]["effects"]["workspace"] = {
            "target": {"kind": "number", "number": 8},
            "activate": False,
        }
        generated = config.render(contract)
        with tempfile.TemporaryDirectory() as directory:
            binding_path = Path(directory) / "bindings.lua"
            binding_path.write_text(generated["hypr/conf/keybinding.lua"])
            observed = capture(binding_path)
            changed = next(row for row in observed if row["keys"] == "SUPER + 8")
            self.assertEqual(
                changed["action"],
                {
                    "method": "focus",
                    "args": [{"workspace": 8, "on_current_monitor": True}],
                },
            )
            rule_path = Path(directory) / "rules.lua"
            rule_path.write_text(generated["hypr/conf/windowrule.lua"])
            observed = capture(rule_path, "rules")[0]
            self.assertEqual(observed["match"]["class"], "^Example$")
            self.assertFalse(observed["float"])
            self.assertEqual(observed["workspace"], "8 silent")

    def test_detached_panels_share_attached_corner_radius_on_both_backends(self):
        row = next(
            row
            for rows in config.load()["outputs"].values()
            for row in rows
            if row.get("selector", {})
            .get("title_regex", "")
            .startswith("^Nacre Pearl ")
        )
        self.assertEqual(row["effects"]["corner_radius"], 18)
        self.assertIn("rounding=18", config.rule_lua(row))
        output = config.niri.rule_kdl(row)
        self.assertIn("geometry-corner-radius 18", output)
        self.assertIn("clip-to-geometry true", output)
        plain = copy.deepcopy(row)
        plain["effects"].pop("corner_radius")
        self.assertNotIn("clip-to-geometry", config.niri.rule_kdl(plain))
        for invalid in (True, -1, 1.5, "18"):
            broken = copy.deepcopy(row)
            broken["effects"]["corner_radius"] = invalid
            with self.subTest(radius=invalid):
                with self.assertRaises(ValueError):
                    config.rule_lua(broken)
                with self.assertRaises(ValueError):
                    config.niri.rule_kdl(broken)

    def test_workspace_targets_keep_monitor_scope_and_dynamic_intent(self):
        self.assertEqual(config.workspace_target({"kind": "number", "number": 3}), 3)
        for scope, expected in (
            ("monitor", "m+1"),
            ("existing", "e+1"),
            ("global", "r+1"),
        ):
            self.assertEqual(
                config.workspace_target(
                    {"kind": "relative", "scope": scope, "offset": 1}
                ),
                expected,
            )
        self.assertEqual(
            config.workspace_target({"kind": "scratchpad", "name": "tools"}),
            "special:tools",
        )
        for value in (
            {"kind": "number", "number": True},
            {"kind": "relative", "scope": "wrong", "offset": 1},
            {"kind": "number", "number": 0},
            {"kind": "number", "number": 3, "extra": 1},
        ):
            with self.subTest(value=value), self.assertRaises(ValueError):
                config.workspace_target(value)

    @unittest.skipUnless(shutil.which("lua"), "Lua unavailable")
    def test_profile_routes_use_same_compiler_and_public_defaults_emit_none(self):
        profile = {"routes": [], "brainWorkspace": None}
        self.assertEqual(config.profile_rules(profile), [])
        profile = {
            "routes": [{"name": "editor", "match": "^Example$", "workspace": 2}],
            "brainWorkspace": 3,
        }
        rows = config.profile_rules(profile)
        self.assertEqual(
            rows[0]["effects"]["workspace"]["target"], {"kind": "number", "number": 2}
        )
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "profile.lua"
            path.write_text("\n".join(config.rule_lua(row) for row in rows))
            rules = capture(path, "rules")
            self.assertEqual(
                [row["workspace"] for row in rules], ["2 silent", "3 silent"]
            )
            self.assertEqual(rules[0]["name"], "profile-editor")

    def test_rejects_unknown_source_version_output_actions_and_extensions(self):
        original = config.load()
        mutations = [
            lambda c: c.update(version=2),
            lambda c: c["outputs"].update({"../private.lua": []}),
            lambda c: c["outputs"]["hypr/conf/keybinding.lua"][0].update(
                action="unsafe.execute"
            ),
            lambda c: c["outputs"]["hypr/conf/keybinding.lua"][0]["parameters"].update(
                injected="code"
            ),
            lambda c: c["extensions"]["panel.drag"].update(source="../../private.lua"),
            lambda c: c["outputs"]["hypr/conf/windowrule.lua"][0]["selector"].update(
                unsupported=True
            ),
            lambda c: c["outputs"]["hypr/conf/windowrule.lua"][0]["effects"].update(
                unsupported=True
            ),
        ]
        for index, mutate in enumerate(mutations):
            with self.subTest(index=index), self.assertRaises(ValueError):
                contract = copy.deepcopy(original)
                mutate(contract)
                config.render(contract)

    def test_missing_or_wrongly_typed_actions_flags_and_rules_fail_before_generation(
        self,
    ):
        mutations = [
            lambda c: next(
                row
                for row in c["outputs"]["hypr/conf/keybinding.lua"]
                if row.get("action") == "workspace.switch"
            ).update(parameters={}),
            lambda c: c["outputs"]["hypr/conf/keybinding.lua"][0]["options"].update(
                locked="true"
            ),
            lambda c: c["outputs"]["hypr/conf/windowrule.lua"][0]["effects"].update(
                floating="false"
            ),
            lambda c: c["outputs"]["hypr/conf/windowrule.lua"][0]["selector"].update(
                app_id_regex=True
            ),
            lambda c: next(
                row
                for row in c["outputs"]["hypr/conf/keybinding.lua"]
                if row.get("action") == "window.resize" and row["parameters"]
            ).update(parameters={"x": True, "y": 0, "relative": True}),
        ]
        for index, mutate in enumerate(mutations):
            with self.subTest(index=index), self.assertRaises(ValueError):
                contract = config.load()
                mutate(contract)
                config.render(contract)
        with self.assertRaises(ValueError):
            config.render(config.load(), backend="unknown")
        with self.assertRaises(ValueError):
            config.rule_lua({}, backend="niri")

    @unittest.skipUnless(shutil.which("lua"), "Lua unavailable")
    def test_strings_cannot_inject_lua_and_unicode_control_chars_roundtrip(self):
        contract = config.load()
        command = 'something "quoted"; error("never run")\n\x01é\\path'
        contract["outputs"]["hypr/conf/keybinding.lua"][0]["parameters"]["command"] = (
            command
        )
        output = config.render(contract)["hypr/conf/keybinding.lua"]
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "bindings.lua"
            path.write_text(output)
            self.assertEqual(capture(path)[0]["action"]["args"], [command])

    @unittest.skipUnless(shutil.which("lua"), "Lua unavailable")
    def test_backend_table_keys_are_quoted_when_not_lua_identifiers(self):
        contract = config.load()
        rows = contract["outputs"]["hypr/conf/nacre.lua"]
        row = next(row for row in rows if row["kind"] == "backend-config")
        row["value"] = {"cursor.no_warps": True, "end": 1, "é": "literal"}
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "backend.lua"
            path.write_text(config.render(contract)["hypr/conf/nacre.lua"])
            declarations = capture(path, "declarations")
            observed = next(row for row in declarations if row["kind"] == "config")
            self.assertEqual(
                observed["value"], {"cursor.no_warps": True, "end": 1, "é": "literal"}
            )

    def test_check_reports_drift_without_modifying_generated_files(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            shutil.copytree(
                ROOT / "tools",
                root / "tools",
                ignore=shutil.ignore_patterns("tests", "__pycache__"),
            )
            helpers = root / "nacre/shell-tools"
            helpers.mkdir(parents=True)
            for file in ("compositor_config.py", "compositor-contract.json"):
                shutil.copy2(DIRECTORY / file, helpers / file)
            shutil.copytree(
                DIRECTORY / "compositor_backends",
                helpers / "compositor_backends",
            )
            generated = config.render(config.load())
            generated.update(config.render(config.load(), backend="niri"))
            for path, output in generated.items():
                target = root / path
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text(output)
            script = root / "tools/generate_compositor_config.py"
            self.assertEqual(
                subprocess.run(
                    [sys.executable, str(script), "--check"], capture_output=True
                ).returncode,
                0,
            )
            changed = root / "hypr/conf/windowrule.lua"
            changed.write_text("-- accidental hand edit\n")
            result = subprocess.run(
                [sys.executable, str(script), "--check"], capture_output=True, text=True
            )
            self.assertEqual(result.returncode, 1)
            self.assertIn("hypr/conf/windowrule.lua", result.stderr)
            self.assertEqual(changed.read_text(), "-- accidental hand edit\n")
            subprocess.run(
                [sys.executable, str(script)], check=True, capture_output=True
            )
            self.assertEqual(
                changed.read_text(),
                config.render(config.load())["hypr/conf/windowrule.lua"],
            )
