"""Welcome preference ownership, login deduplication and single-shell activation."""

import importlib.machinery
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import Mock, patch
from concurrent.futures import ThreadPoolExecutor
import contextlib
import io
import shutil

ROOT = Path(__file__).parents[3]


def load(name, path):
    loader = importlib.machinery.SourceFileLoader(name, str(path))
    spec = importlib.util.spec_from_loader(name, loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


welcome = load("welcome_owner", ROOT / "nacre/shell-tools/welcome.py")
launcher = load("welcome_launcher", ROOT / "bin/nacre-welcome")
configure = load("welcome_configure", ROOT / "tools/configure.py")


class WelcomeTests(unittest.TestCase):
    def test_disabled_login_and_manual_reads_do_not_reenable(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            self.assertTrue(welcome.preferences(home)["showAtLogin"])
            self.assertFalse(welcome.paths(home)[0].exists())
            welcome.set_startup(home, False)
            before = welcome.paths(home)[0].read_bytes()
            self.assertFalse(welcome.claim_login(home, "session-1")["show"])
            self.assertFalse(welcome.state(home)["preferences"]["showAtLogin"])
            self.assertEqual(before, welcome.paths(home)[0].read_bytes())
            self.assertEqual(welcome.paths(home)[0].stat().st_mode & 0o777, 0o600)

    def test_claim_once_per_session_and_failed_activation_retry(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            first = welcome.claim_login(home, "one")
            self.assertTrue(first["show"])
            self.assertFalse(welcome.claim_login(home, "one")["show"])
            welcome.release_login(home, "one", "other-claim")
            self.assertFalse(welcome.claim_login(home, "one")["show"])
            welcome.release_login(home, "one", first["claim"])
            self.assertTrue(welcome.claim_login(home, "one")["show"])
            self.assertTrue(welcome.claim_login(home, "two")["show"])
            self.assertFalse(welcome.claim_login(home, "")["show"])

    def test_first_run_is_consumed_only_after_success_and_stale_claim_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            first = welcome.claim_login(home, "one")
            with self.assertRaises(ValueError):
                welcome.acknowledge_login(home, "one", "not-ours")
            self.assertNotIn("firstRunShown", welcome.preferences(home))
            welcome.acknowledge_login(home, "one", first["claim"])
            self.assertTrue(welcome.preferences(home)["firstRunShown"])
            self.assertFalse(welcome.claim_login(home, "two")["show"])
            self.assertFalse(welcome.claim_login(home, "one")["show"])

    def test_completed_manual_tour_also_suppresses_automatic_startup(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            welcome.atomic(
                welcome.paths(home)[0], {"version": 1, "tourCompleted": True}
            )
            self.assertFalse(welcome.claim_login(home, "next-login")["show"])

    def test_unknown_fields_preserved_and_bad_configuration_is_not_overwritten(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            path, _ = welcome.paths(home)
            path.parent.mkdir(parents=True)
            path.write_text(json.dumps({"version": 1, "custom": "preserve"}))
            welcome.set_startup(home, False)
            self.assertEqual(welcome.preferences(home)["custom"], "preserve")
            for text in ('{"showAtLogin":"no"}', '{"version":2}', "broken json"):
                path.write_text(text)
                with self.assertRaises(ValueError):
                    welcome.set_startup(home, True)
                self.assertEqual(path.read_text(), text)
            with self.assertRaises(ValueError):
                welcome.set_startup(home, "false")

    def test_warm_activation_does_not_start_or_restart_services(self):
        runner = Mock(return_value=subprocess.CompletedProcess([], 0, "", ""))
        launcher.open_welcome("shortcuts", runner=runner)
        self.assertEqual(runner.call_count, 1)
        self.assertEqual(
            runner.call_args.args[0][-3:], ["welcomeApp", "open", "shortcuts"]
        )

    def test_cold_activation_starts_one_existing_service_with_bounded_retry(self):
        bad = subprocess.CompletedProcess([], 1, "", "")
        good = subprocess.CompletedProcess([], 0, "", "")
        runner = Mock(side_effect=[bad, good, bad, good])
        sleep = Mock()
        launcher.open_welcome(runner=runner, sleep=sleep)
        self.assertEqual(
            runner.call_args_list[1].args[0],
            ["systemctl", "--user", "start", "--no-block", "nacre-shell.service"],
        )
        self.assertEqual(sleep.call_count, 2)
        runner = Mock(return_value=bad)
        with self.assertRaises(RuntimeError):
            launcher.open_welcome(runner=runner, sleep=Mock())
        self.assertEqual(runner.call_count, 2)

    def test_concurrent_login_claims_activate_only_once(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            with ThreadPoolExecutor(max_workers=4) as pool:
                results = list(
                    pool.map(lambda _: welcome.claim_login(home, "same"), range(4))
                )
            self.assertEqual(sum(item["show"] for item in results), 1)

    def test_disabled_login_cli_does_not_start_shell_or_open_window(self):
        with (
            patch("sys.argv", ["nacre-welcome", "--login"]),
            patch.object(launcher, "helper", return_value={"show": False, "claim": ""}),
            patch.object(launcher, "open_welcome") as opener,
        ):
            self.assertEqual(launcher.main(), 0)
            opener.assert_not_called()

    def test_successful_first_login_acknowledges_after_opening(self):
        events = []
        with (
            patch("sys.argv", ["nacre-welcome", "--login"]),
            patch.object(
                launcher,
                "helper",
                side_effect=lambda action, *args: (
                    events.append(action) or {"show": True, "claim": "ours"}
                ),
            ),
            patch.object(
                launcher,
                "open_welcome",
                side_effect=lambda *args: events.append("open"),
            ),
        ):
            self.assertEqual(launcher.main(), 0)
        self.assertEqual(events, ["claim-login", "open", "acknowledge-login"])

    def test_failed_login_cli_releases_its_claim(self):
        with (
            patch("sys.argv", ["nacre-welcome", "--login"]),
            patch.object(
                launcher,
                "helper",
                side_effect=[{"show": True, "claim": "ours"}, {"released": True}],
            ) as helper,
            patch.object(
                launcher, "open_welcome", side_effect=RuntimeError("unavailable")
            ),
            contextlib.redirect_stderr(io.StringIO()),
        ):
            self.assertEqual(launcher.main(), 1)
            self.assertEqual(helper.call_args.args, ("release-login", "ours"))

    def test_live_shortcuts_follow_remaps_and_omit_removed_actions(self):
        bindings = [
            {"key": "E", "modmask": 68, "description": "Nacre:files", "submap": ""},
            {"key": "F", "modmask": 64, "description": "", "arg": "private command"},
            {
                "key": "R",
                "modmask": 64,
                "description": "Nacre:files",
                "submap": "resize",
            },
        ]
        rows = welcome.shortcut_rows(bindings)
        self.assertEqual(len(rows), 1)
        self.assertEqual(rows[0]["key"], "Super + Ctrl + E")
        self.assertNotIn("private", json.dumps(rows))
        self.assertEqual(welcome.shortcut_rows([]), [])

    def test_live_shortcut_ranges_aliases_and_related_keys(self):
        binds = [
            {"key": str(n), "modmask": 64, "description": "Nacre:workspace"}
            for n in range(2, 6)
        ]
        binds += [
            {"key": str(n), "modmask": 65, "description": "Nacre:move-workspace"}
            for n in range(2, 6)
        ]
        binds += [
            {"key": key, "modmask": 64, "description": "Nacre:focus"}
            for key in ["left", "right", "up", "down"]
        ]
        binds += [
            {"key": "Q", "modmask": 64, "description": "Nacre:put-away"},
            {"key": "X", "modmask": 65, "description": "Nacre:restore"},
            {"key": "C", "modmask": 68, "description": "Nacre:close"},
        ]
        rows = {row["id"]: row for row in welcome.shortcut_rows(binds)}
        self.assertEqual(rows["workspace"]["key"], "Super + 2–5")
        self.assertIn("Super + Shift + 2–5", rows["workspace"]["detail"])
        self.assertEqual(rows["focus"]["key"], "Super + arrows")
        self.assertIn("Super + Shift + X", rows["put-away"]["detail"])
        self.assertNotIn("Shift + Q", rows["put-away"]["detail"])
        binds.pop(0)
        self.assertEqual(welcome.binding_labels(binds)["workspace"], "Super + 3–5")
        with self.assertRaises(ValueError):
            welcome.shortcut_rows({})

    def test_no_compositor_has_an_explicit_unavailable_state(self):
        with patch.object(welcome.subprocess, "run", side_effect=FileNotFoundError()):
            rows, error = welcome.live_shortcuts()
        self.assertEqual(rows, [])
        self.assertIn("Hyprland", error)

    @unittest.skipUnless(shutil.which("lua"), "Lua unavailable")
    def test_actual_lua_bind_registrations_have_descriptions(self):
        script = """local function proxy(path)
return setmetatable({}, {__index=function(t,k) local p=proxy(path..'.'..k);rawset(t,k,p);return p end,__call=function(t,...) return {} end}) end
hl={window_rule=function(rule) assert(type(rule)=='table') end,dsp=proxy('hl'),bind=function(keys,action,options) if options and options.description then io.write(keys..'\\t'..options.description..'\\n') end end,define_submap=function(name,fn) fn() end}
dofile(arg[1]);dofile(arg[2])
"""
        result = subprocess.run(
            [
                "lua",
                "-",
                str(ROOT / "hypr/conf/keybinding.lua"),
                str(ROOT / "nacre/shell-tools/shortcuts.lua"),
            ],
            input=script,
            text=True,
            capture_output=True,
            check=True,
        )
        rows = dict(line.split("\t") for line in result.stdout.splitlines())
        self.assertEqual(rows["SUPER + A"], "Nacre:launcher")
        self.assertEqual(rows["SUPER + SHIFT + F"], "Nacre:files")
        self.assertEqual(rows["SUPER + CTRL + B"], "Nacre:ai-sidebar")
        self.assertEqual(rows["SUPER + CTRL + D"], "Nacre:detach-panel")
        self.assertEqual(rows["SUPER + 7"], "Nacre:workspace")

    def test_deployment_and_login_owner_are_declared(self):
        mapping = configure.files(ROOT)
        self.assertIn(Path(".local/bin/nacre-welcome"), mapping)
        self.assertIn(Path(".local/share/applications/nacre-welcome.desktop"), mapping)
        entry = (ROOT / "nacre/desktop/nacre-welcome.desktop").read_text()
        self.assertIn("Icon=nacre\n", entry)
        self.assertIn("Terminal=false", entry)
        startup = (ROOT / "hypr/conf/autostart.lua").read_text()
        self.assertEqual(startup.count('"--login"'), 1)
        self.assertNotIn("--login", (ROOT / "nacre/shell/shell.qml").read_text())
