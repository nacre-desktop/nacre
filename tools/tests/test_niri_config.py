"""Shared neutral-source lowering, safe fragments and native niri validation."""

import copy
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "nacre/shell-tools"))
import nacre_native_config as config

niri = config.niri


class NiriConfigTests(unittest.TestCase):
    def test_tabs_capture_cycle_and_floating_sizes_are_lowered(self):
        generated = config.render(config.load(), backend="niri")
        binds = generated["niri/bindings.kdl"]
        for action in (
            "toggle-column-tabbed-display",
            "focus-window-up-or-bottom",
            "focus-window-down-or-top",
            "cycle_window --direction next",
            "screenshot-screen --show-pointer false --path",
        ):
            self.assertIn(action, binds)
        rules = generated["niri/window-rules.kdl"]
        self.assertIn("default-column-width { fixed 700; }", rules)
        self.assertIn("default-window-height { proportion 0.75; }", rules)
        self.assertIn("background-effect { blur true; }", rules)
        unsupported = json.loads(generated["niri/unsupported.json"])["unsupported"]
        self.assertFalse(any("group." in row["reason"] for row in unsupported))
        self.assertTrue(any("scratchpad" in row["reason"] for row in unsupported))

    def test_hyprland_bytes_and_native_registration_contract_remain_unchanged(self):
        original = config.render(config.load(), backend="hyprland")
        self.assertEqual(set(original), set(config.OUTPUTS))
        for name, text in original.items():
            self.assertEqual(text, (ROOT / name).read_text(), name)
        generated = config.render(config.load(), backend="niri")
        self.assertFalse(set(generated) & set(original))
        self.assertEqual(original, config.render(config.load(), backend="hyprland"))

    def test_bindings_use_facade_and_output_relative_workspace_targets(self):
        generated = config.render(config.load(), backend="niri")
        text = generated["niri/bindings.kdl"]
        self.assertIn("NACRE_COMPOSITOR=niri", text)
        self.assertIn("nacre-helper", text)
        self.assertNotIn("python3", text)
        self.assertIn("NACRE_COMPOSITOR_TOOLS", text)
        self.assertIn("switch_workspace --workspace 2", text)
        self.assertIn("move_window --workspace 2", text)
        for direction in ("left", "right"):
            self.assertIn(
                f'Super+{direction.title()} repeat=true cooldown-ms=120 hotkey-overlay-title="Nacre:focus" '
                f"{{ focus-column-{direction}; }}",
                text,
            )
            self.assertNotIn("focus_direction --direction " + direction, text)
        self.assertNotIn("hyprctl", text)
        self.assertIn("action close_window", text)
        self.assertIn('Super+Q repeat=false hotkey-overlay-title="Nacre:close"', text)
        self.assertNotIn("Super+Ctrl+Q ", text)
        self.assertNotIn("window-close.sh", text)
        self.assertIn("niri msg action toggle-overview", text)
        self.assertNotIn("nacre-shell overview", text)
        unsupported = json.loads(generated["niri/unsupported.json"])["unsupported"]
        self.assertFalse(any(row.get("keys") == "SUPER + Q" for row in unsupported))
        self.assertFalse(
            any(
                row.get("description")
                in {
                    "Nacre:put-away",
                    "Nacre:restore",
                    "Nacre:restore-all",
                    "Nacre:force-close",
                }
                for row in unsupported
            )
        )
        self.assertTrue(any("window.pin" in row["reason"] for row in unsupported))
        self.assertTrue(any("pointer" in row["reason"] for row in unsupported))

    def test_pearl_rule_and_orient_colors_do_not_replace_user_settings(self):
        rules = config.render(config.load(), backend="niri")["niri/window-rules.kdl"]
        self.assertIn("Nacre Pearl (Launcher|Dashboard|AI|Controls|Media)", rules)
        self.assertIn("open-floating true", rules)
        palette = (
            ROOT / "nacre/shell-tools/compositor_backends/niri/palette.kdl.in"
        ).read_text()
        self.assertIn("{{primary}}", palette)
        self.assertIn("{{secondary}}", palette)
        self.assertIn("{{outlineVariant}}", palette)
        self.assertIn("{{error}}", palette)
        self.assertNotIn("width ", palette)
        self.assertNotIn("\n        on", palette)
        self.assertNotIn("\n        off", palette)

    def test_active_gradients_keep_hyprland_role_order_and_angle(self):
        directory = ROOT / "nacre/shell-tools/compositor_backends"
        hyprland = (directory / "hyprland/palette.lua.in").read_text()
        roles = re.findall(r"rgba\(\{\{(\w+)\}\}ff\)", hyprland)
        angle = re.search(r"angle=(\d+)", hyprland).group(1)
        palette = (directory / "niri/palette.kdl.in").read_text()
        gradients = re.findall(
            r'active-gradient from="#\{\{(\w+)\}\}" '
            r'to="#\{\{(\w+)\}\}" angle=(\d+)',
            palette,
        )
        self.assertEqual(roles, ["primary", "secondary"])
        self.assertEqual(gradients, [(*roles, angle), (*roles, angle)])

    def test_new_neutral_binding_is_reflected_in_both_backends(self):
        contract = copy.deepcopy(config.load())
        row = next(
            row
            for row in contract["outputs"]["hypr/conf/keybinding.lua"]
            if row["action"] == "workspace.switch"
            and row["parameters"]["workspace"].get("number") == 2
        )
        row["keys"] = "SUPER + CTRL + ALT + F12"
        row["parameters"]["workspace"]["number"] = 42
        self.assertIn(
            "switch_workspace --workspace 42",
            config.render(contract, backend="niri")["niri/bindings.kdl"],
        )
        self.assertIn(
            "SUPER + CTRL + ALT + F12",
            config.render(contract, backend="hyprland")["hypr/conf/keybinding.lua"],
        )

    def test_source_shortcuts_honor_explicit_config_and_binary_overrides(self):
        text = niri.binding_command(
            {
                "kind": "binding",
                "action": "shell.shortcut",
                "parameters": {"name": "nacre_shell:test"},
            }
        )
        self.assertIn("NACRE_QS_CONFIG", text)
        self.assertIn("NACRE_QS_BIN", text)
        self.assertIn('-p "$NACRE_QS_CONFIG"', text)
        self.assertIn("nacreCompositor shortcut", text)

    def test_profile_workspace_rules_require_explicit_names_on_niri(self):
        rule = {
            "kind": "window-rule",
            "selector": {"app_id_regex": "^Example$"},
            "effects": {
                "workspace": {
                    "target": {"kind": "number", "number": 2},
                    "activate": False,
                }
            },
        }
        with self.assertRaises(NotImplementedError):
            config.rule_kdl(rule)
        rule["effects"]["workspace"]["target"] = {"kind": "name", "name": "work"}
        self.assertIn('open-on-workspace "work"', config.rule_kdl(rule))

    def test_public_keyboard_profile_api_and_setup_gate(self):
        self.assertEqual(config.supported_backends(), ("hyprland", "niri"))
        self.assertFalse(config.backend_capabilities("niri")["setup_ready"])
        self.assertTrue(config.backend_capabilities("hyprland")["setup_ready"])
        self.assertIn(
            'layout "us"', config.render_keyboard({"layout": "us"}, backend="niri")
        )
        self.assertIn(
            "input.kb_layout",
            config.render_keyboard({"layout": "us"}, backend="hyprland"),
        )
        profile = {"routes": [], "brainWorkspace": None}
        self.assertEqual(config.render_profile(profile, backend="niri"), "")
        profile["routes"] = [{"name": "editor", "match": "^Example$", "workspace": 2}]
        with self.assertRaises(ValueError):
            config.render_profile(profile, backend="niri")
        self.assertIn(
            'open-on-workspace "work"',
            config.render_profile(profile, backend="niri", workspace_names={2: "work"}),
        )

    def test_untrusted_values_are_quoted_and_unknown_actions_reported(self):
        text = niri.rule_kdl(
            {
                "kind": "window-rule",
                "selector": {"title_regex": 'name"\n} binds {'},
                "effects": {"floating": True},
            }
        )
        self.assertIn('name\\"\\n} binds {', text)
        self.assertEqual(niri.quoted(r"literal\u0041"), '"literal\\\\u0041"')
        self.assertEqual(niri.quoted("\x01"), '"\\u{1}"')
        with self.assertRaises(ValueError):
            niri.quoted("nul\0")
        with self.assertRaises(ValueError):
            niri.keys("Super + Return }")
        with self.assertRaises(ValueError):
            config.render(
                {"version": 1, "outputs": {"fixture": [{"kind": "unknown"}]}},
                backend="niri",
            )

    @unittest.skipUnless(
        shutil.which("niri"),
        "Native niri validator unavailable; deterministic checks still run",
    )
    def test_native_26_04_parser_and_include_precedence(self):
        generated = config.render(config.load(), backend="niri")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "bindings.kdl").write_text(generated["niri/bindings.kdl"])
            (root / "rules.kdl").write_text(generated["niri/window-rules.kdl"])
            (root / "wallpaper.kdl").write_text(
                (ROOT / "niri/wallpaper.kdl").read_text()
            )
            palette = (
                ROOT / "nacre/shell-tools/compositor_backends/niri/palette.kdl.in"
            ).read_text()
            for role, color in {
                "primary": "123456",
                "secondary": "abcdef",
                "outlineVariant": "657483",
                "error": "d80f4a",
            }.items():
                palette = palette.replace("{{" + role + "}}", color)
            (root / "colors.kdl").write_text(palette)
            # Includes precede user-owned choices: duplicate keys across files
            # are explicitly merged by niri, while user settings remain intact.
            text = 'include "wallpaper.kdl"\ninclude "bindings.kdl"\ninclude "rules.kdl"\ninclude "colors.kdl"\nlayout { gaps 13; border { width 2; active-color "#abcdef"; }; }\nbinds { Super+2 { focus-workspace 1; }; }\n'
            path = root / "config.kdl"
            path.write_text(text)
            result = subprocess.run(
                ["niri", "validate", "-c", str(path)],
                capture_output=True,
                text=True,
                timeout=10,
            )
            self.assertEqual(
                result.returncode, 0, (result.stdout + result.stderr)[-5000:]
            )
            self.assertEqual(path.read_text(), text)


if __name__ == "__main__":
    unittest.main()
