"""N0048: every `nacre` command and compositor binding reaches the shell through one route."""

import json
from pathlib import Path
import re
import shlex
import sys
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "nacre/shell-tools"))
import nacre_native_config as config

REGISTRY = json.loads((ROOT / "nacre/shell-tools/shell-routes.json").read_text())
ROUTES = {row["id"]: row for row in REGISTRY["routes"]}
# Window rules match the class `org.quickshell`; that is not an IPC call.
FORBIDDEN = ("quickshell ipc", "qs ipc", "ipc call", "bin/qs", "qs -c")
BINDING_FILES = (
    "niri/bindings.kdl",
    "hypr/conf/keybinding.lua",
    "hypr/conf/nacre.lua",
    "hypr/conf/brain.lua",
    "nacre/shell-tools/shortcuts.lua",
)


def ui_calls(text):
    """(target, method, args) for each `nacrectl ui ...` occurrence in generated text."""
    calls = []
    for line in text.splitlines():
        if "nacrectl" not in line:
            continue
        if line.lstrip().startswith("hl."):
            match = re.search(r'exec_cmd\("((?:[^"\\]|\\.)*)"\)', line)
            words = shlex.split(match.group(1)) if match else []
        else:
            match = re.search(r"spawn((?: \"(?:[^\"\\]|\\.)*\")+)", line)
            words = shlex.split(match.group(1)) if match else []
        if "nacrectl" in words:
            words = words[words.index("nacrectl") + 1 :]
        if words[:1] == ["ui"]:
            calls.append(tuple(words[1:]))
    return calls


def registered(call):
    route = ROUTES.get(f"{call[0]}.{call[1]}")
    return route is not None and any(len(call) - 2 == len(e) for e in route["examples"])


class ShellRouteTests(unittest.TestCase):
    def test_every_control_command_has_a_registry_row(self):
        text = (ROOT / "nacre/shell-tools/control.sh").read_text()
        cases = set()
        for label in re.findall(r"^ ([a-z|-]+)\)", text, re.MULTILINE):
            cases.update(label.split("|"))
        rows = {row["command"] for row in REGISTRY["commands"]}
        self.assertEqual(cases - rows, set())
        self.assertEqual(rows - cases, set())
        self.assertEqual(len(rows), len(REGISTRY["commands"]))

    def test_every_ui_command_resolves_to_a_route_and_control_uses_it(self):
        text = (ROOT / "nacre/shell-tools/control.sh").read_text()
        for row in REGISTRY["commands"]:
            if row["kind"] == "ui":
                self.assertIn(row["route"], ROUTES, row["command"])
        self.assertNotIn("quickshell", text)
        self.assertNotIn(" ipc ", text)
        # Literal targets only; brightness/keyboard-light share one case through "$target".
        for call in re.findall(r'nacrectl" ui ([A-Za-z]+) ([A-Za-z]+)', text):
            self.assertIn(".".join(call), ROUTES)
        self.assertIn('ui "$target" step', text)
        self.assertIn("brightness.step", ROUTES)
        self.assertIn("keyboardLight.step", ROUTES)
        for row in REGISTRY["apps"]:
            self.assertIn(row["route"], ROUTES)

    def test_qml_only_rows_carry_an_expiry(self):
        for row in REGISTRY["commands"]:
            if row["kind"] == "qml-only":
                self.assertTrue(row["expiry"])
        for row in REGISTRY["fallbacks"]:
            self.assertTrue(row["expiry"])

    def test_gap_rows_name_the_bead_unit_to_do(self):
        for row in ROUTES.values():
            self.assertIn(row["bead"], ("done", "partial", "gap"))
            if row["bead"] == "gap":
                self.assertTrue(row["spec"].startswith("Bead:"), row["id"])
            else:
                self.assertTrue(row["spec"], row["id"])

    def test_generated_bindings_use_only_registered_ui_routes(self):
        seen = set()
        for backend in ("hyprland", "niri"):
            contract = config.load()
            for name, text in config.render(contract, backend=backend).items():
                for forbidden in FORBIDDEN:
                    self.assertNotIn(forbidden, text, name)
                for call in ui_calls(text):
                    self.assertTrue(registered(call), (name, call))
                    seen.add(call[:2])
                if name.endswith("keybinding.lua") or name.endswith("bindings.kdl"):
                    for method in (
                        "audio.step",
                        "brightness.adjust",
                        "keyboard_light.adjust",
                    ):
                        self.assertEqual(
                            text.count(f"nacrectl call {method}"), 2, (backend, method)
                        )
        # Hardware key steps are daemon calls; brightness UI routes remain for manual controls.
        self.assertIn(("nacre", "launcher"), seen)
        self.assertNotIn(("brightness", "step"), seen)

    def test_committed_binding_files_use_only_registered_ui_routes(self):
        for name in BINDING_FILES:
            text = (ROOT / name).read_text()
            for forbidden in FORBIDDEN:
                self.assertNotIn(forbidden, text, name)
            for call in ui_calls(text):
                self.assertTrue(registered(call), (name, call))

    def test_qml_provider_answers_every_registered_route(self):
        sources = "\n".join(
            path.read_text() for path in (ROOT / "nacre/shell").rglob("*.qml")
        )
        handlers = {}
        for target, body in re.findall(
            r'registerIpc\("([A-Za-z]+)", \w+, \{(.*?)\}\)', sources, re.DOTALL
        ):
            for method, signature in re.findall(r'"(\w+)": \[([^\]]*)\]', body):
                handlers[f"{target}.{method}"] = signature.count('"') // 2
        for route in ROUTES.values():
            self.assertIn(route["id"], handlers, route["id"])
            self.assertTrue(
                (ROOT / "nacre/shell" / "services" / route["qml"]).exists()
                or any((ROOT / "nacre/shell").rglob(route["qml"])),
                route["qml"],
            )
            counts = {len(example) for example in route["examples"]}
            if route["id"] == "nacre.settings":
                # The one-argument form is answered in NacreShellIpc.daemonCall.
                self.assertIn("openSettings(params.args[0])", sources)
                counts.discard(1)
            self.assertEqual(counts, {handlers[route["id"]]}, route["id"])

    def test_registry_paths_exist(self):
        for row in REGISTRY["routes"]:
            self.assertTrue(list((ROOT / "nacre/shell").rglob(row["qml"])), row["qml"])


if __name__ == "__main__":
    unittest.main()
