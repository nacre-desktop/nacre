"""Harness integrity, negative controls, framing and transport equivalence."""

import importlib.util
import copy
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import Mock, call, patch

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("parity_runner", HERE / "run.py")
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


class RunnerTests(unittest.TestCase):
    def test_profile_install_reference_pairs_only_new_private_lease_metadata(self):
        case = json.loads((HERE / "profile-policy/case-install.json").read_text())
        expected = json.loads(
            (HERE / "profile-policy/expected-install.json").read_text()
        )
        frozen = copy.deepcopy(expected)
        actual = copy.deepcopy(expected)
        actual["files"]["home/.config"]["mode"] = "0o700"
        actual["files"]["home/.config/nacre"]["mode"] = "0o700"
        actual["files"]["home/.config/nacre/.writer.lock"] = {
            "mode": "0o600",
            "text": "",
        }

        paired = runner.profile_install_reference(expected, actual, case)
        self.assertEqual(paired, actual)
        self.assertEqual(expected, frozen)

        for mutation in (
            lambda value: value["files"]["home/.config"].update(mode="0o755"),
            lambda value: value["files"].pop("home/.config/nacre/.writer.lock"),
            lambda value: value["files"]["home/.config/nacre/.writer.lock"].update(
                text="unexpected"
            ),
            lambda value: value["files"].update(unrelated={"mode": "0o600"}),
        ):
            changed = copy.deepcopy(actual)
            mutation(changed)
            with self.subTest(changed=changed["files"]):
                try:
                    paired_changed = runner.profile_install_reference(
                        expected, changed, case
                    )
                except ValueError:
                    continue
                self.assertNotEqual(paired_changed, changed)

        for wrong_case in (
            {**case, "args": ["show"]},
            {**case, "helper": "other"},
            {**case, "fixtures": {"home/file": "data"}},
        ):
            self.assertEqual(
                runner.profile_install_reference(expected, actual, wrong_case),
                expected,
            )

    def test_supervisor_timeout_identifies_the_case_and_parent_state(self):
        from supervisor_reference import capture

        for status in (None, 23):
            with self.subTest(parent_exit=status):
                child = Mock()
                child.poll.return_value = status
                child.returncode = status
                if status is None:
                    child.wait.side_effect = [
                        subprocess.TimeoutExpired("fixture", 60),
                        None,
                    ]

                def spawn(*args, child=child, **kwargs):
                    root = Path(kwargs["env"]["HOME"]) / ".local/share/nacre/shell"
                    (root / "events").write_text("start healthy\nchild-running\n")
                    return child

                with (
                    patch("supervisor_reference.subprocess.Popen", side_effect=spawn),
                    patch.dict(os.environ, NACRE_HELPER_BINARY="/fixture/nacre-helper"),
                ):
                    if status is None:
                        with self.assertRaisesRegex(
                            AssertionError,
                            "Supervisor case clean timed out after 60 seconds; parent_exit=None",
                        ) as caught:
                            capture(["fixture"], names=("clean",))
                        self.assertIsInstance(
                            caught.exception.__cause__, subprocess.TimeoutExpired
                        )
                        self.assertIn("child-running", str(caught.exception))
                    else:
                        result = capture(["fixture"], names=("clean",))[0]
                        self.assertEqual(result["exit"], status)
                child.wait.assert_any_call(timeout=60)
                if status is None:
                    child.kill.assert_called_once_with()
                    self.assertEqual(
                        child.wait.call_args_list, [call(timeout=60), call()]
                    )
                else:
                    child.kill.assert_not_called()
                    child.wait.assert_called_once_with(timeout=60)

    def test_account_help_extensions_are_exact_and_preserve_other_observations(self):
        for helper in ("nacre-ai", "nacre-ai-chat"):
            case = json.loads((HERE / helper / "case-help.json").read_text())
            frozen = json.loads((HERE / helper / "expected-help.json").read_text())
            original = copy.deepcopy(frozen)
            expected = runner.account_help_reference(frozen, case)
            self.assertEqual(frozen, original)
            if helper == "nacre-ai":
                self.assertIn("Codex accounts share chats", expected["stdout"])
            else:
                self.assertIn("  --all-accounts\n", expected["stdout"])
                self.assertIn("  --history-home HISTORY_HOME\n", expected["stdout"])
            for key, value in [
                ("stdout", expected["stdout"] + "extra\n"),
                ("exit", 1),
                ("files", {"unexpected": {}}),
            ]:
                changed = {**expected, key: value}
                self.assertNotEqual(
                    runner.account_help_reference(frozen, case), changed
                )
            self.assertEqual(
                runner.account_help_reference(frozen, {"helper": helper}), frozen
            )

    def test_account_help_frozen_outputs_cannot_be_re_recorded(self):
        for helper in ("nacre-ai", "nacre-ai-chat"):
            reference = HERE / helper / "expected-help.json"
            before = reference.read_bytes()
            result = subprocess.run(
                [
                    sys.executable,
                    str(HERE / "run.py"),
                    "--record",
                    "--case",
                    helper + "/help",
                ],
                text=True,
                capture_output=True,
                timeout=5,
            )
            self.assertEqual(result.returncode, 2)
            message = (
                "Migrated helper references cannot be re-recorded: " + helper
                if helper in runner.native_registry.MIGRATED
                else "Frozen account-help cases cannot be re-recorded"
            )
            self.assertIn(message, result.stderr)
            self.assertEqual(reference.read_bytes(), before)

    def test_account_help_rule_rejects_wrong_scope_and_changed_reference(self):
        case = json.loads((HERE / "nacre-ai" / "case-help.json").read_text())
        frozen = json.loads((HERE / "nacre-ai" / "expected-help.json").read_text())
        for changed in [
            {**case, "helper": "other"},
            {**case, "args": ["new"]},
            {**case, "account_help": "anything"},
            {**case, "stdout": "json"},
        ]:
            with self.assertRaisesRegex(ValueError, "Unsupported account-help"):
                runner.account_help_reference(frozen, changed)
        with self.assertRaisesRegex(ValueError, "Frozen account-help"):
            runner.account_help_reference({**frozen, "stdout": "unexpected"}, case)

    def test_historical_catalog_assets_do_not_mask_current_bundle_inputs(self):
        case = {"helper": "wallpaper-media", "args": ["catalog"]}
        self.assertEqual(
            runner.historical_catalog_mounts(case),
            ["--tmpfs", "/repo/nacre/shell-tools/theme-bundles"],
        )
        self.assertEqual(
            runner.historical_catalog_mounts(
                {**case, "env": {"EXPLICIT_BUNDLE_INPUT": "1"}}
            ),
            [],
        )
        self.assertEqual(
            runner.historical_catalog_mounts({"helper": "other", "args": ["catalog"]}),
            [],
        )
        frozen = runner.execute(case, runner.native_registry.command(case, None))
        self.assertEqual(frozen["exit"], 0)
        self.assertFalse(
            any(row["id"].startswith("bundle:") for row in frozen["stdout"]["palettes"])
        )
        current_case = {**case, "env": {"EXPLICIT_BUNDLE_INPUT": "1"}}
        current = runner.execute(
            current_case, runner.native_registry.command(current_case, None)
        )
        self.assertEqual(current["exit"], 0)
        self.assertTrue(
            any(
                row["id"].startswith("bundle:") for row in current["stdout"]["palettes"]
            )
        )

    def test_automatic_wallpaper_default_never_erases_enabled_difference(self):
        old = {
            "stdout": {
                "preferences": {"paletteMode": "dark", "palettePreset": "wallpaper"}
            },
            "files": {
                "home/.config/nacre/wallpaper-picker.json": {
                    "mode": "0o600",
                    "text": '{"paletteMode":"dark"}',
                }
            },
        }
        new = copy.deepcopy(old)
        new["stdout"]["preferences"]["paletteAutoMode"] = False
        new["files"]["home/.config/nacre/wallpaper-picker.json"]["text"] = (
            '{"paletteMode":"dark","paletteAutoMode":false}'
        )
        case = {"helper": "wallpaper-media"}
        self.assertEqual(
            runner.compatible_files(old, case), runner.compatible_files(new, case)
        )
        new["stdout"]["preferences"]["paletteAutoMode"] = True
        self.assertNotEqual(
            runner.compatible_files(old, case), runner.compatible_files(new, case)
        )

    def test_hosted_traceback_contracts_keep_every_other_observable_field(self):
        for helper, number in [
            ("dashboard-data", "10"),
            ("dashboard-media", "00"),
            ("notification-history", "00"),
            ("notification-history", "05"),
            ("notification-history", "06"),
        ]:
            path = HERE / helper / f"case-cli-boundary-{number}.json"
            case = json.loads(path.read_text())
            expected = json.loads(
                path.with_name(path.name.replace("case-", "expected-")).read_text()
            )
            actual = {**expected, "stderr": case["native_exception"]["terminal_line"]}
            original = copy.deepcopy(expected)
            with self.subTest(helper=helper, number=number):
                self.assertEqual(
                    runner.native_exception_reference(expected, actual, case), actual
                )
                self.assertEqual(expected, original)
                changed = {**actual, "files": {"unexpected": True}}
                self.assertNotEqual(
                    runner.native_exception_reference(expected, changed, case), changed
                )
                with self.assertRaisesRegex(ValueError, "exact declared"):
                    runner.native_exception_reference(
                        expected, actual, {**case, "args": ["other"]}
                    )

    def test_native_exception_omits_only_declared_retired_source_frames(self):
        case = json.loads(
            (HERE / "menu-backend/case-cli-reminder-default.json").read_text()
        )
        expected = json.loads(
            (HERE / "menu-backend/expected-cli-reminder-default.json").read_text()
        )
        actual = {**expected, "stderr": case["native_exception"]["terminal_line"]}
        original = copy.deepcopy(expected)
        self.assertEqual(
            runner.native_exception_reference(expected, actual, case), actual
        )
        self.assertEqual(expected, original)
        for field, value in (
            ("stdout", {"error": "changed"}),
            ("exit", 0),
            ("files", {}),
        ):
            changed = {**actual, field: value}
            self.assertNotEqual(
                runner.native_exception_reference(expected, changed, case), changed
            )

    def test_native_exception_refuses_wrong_message_source_action_or_traceback(self):
        case = json.loads(
            (HERE / "menu-backend/case-cli-reminder-default.json").read_text()
        )
        expected = json.loads(
            (HERE / "menu-backend/expected-cli-reminder-default.json").read_text()
        )
        actual = {**expected, "stderr": case["native_exception"]["terminal_line"]}
        for field, value in (
            ("helper", "dashboard-data"),
            ("args", ["reminder", "other"]),
            ("stdin", '{"minutes": 1}\n'),
            ("reference_source_sha256", "wrong"),
            ("native_exception", {"kind": "anything"}),
        ):
            with (
                self.subTest(field=field),
                self.assertRaisesRegex(ValueError, "exact declared"),
            ):
                runner.native_exception_reference(
                    expected, actual, {**case, field: value}
                )
        for candidate in (
            {**actual, "stderr": "TypeError: wrong\n"},
            {**actual, "stderr": ""},
        ):
            with self.assertRaisesRegex(ValueError, "exact declared"):
                runner.native_exception_reference(expected, candidate, case)
        for frozen in ({**expected, "stderr": "unrelated\n"}, {**expected, "exit": 0}):
            with self.assertRaisesRegex(ValueError, "exact declared"):
                runner.native_exception_reference(frozen, actual, case)

    def test_native_service_fixture_retains_the_frozen_provider_and_mode(self):
        case = json.loads((HERE / "helper-service/case-hosted-crash.json").read_text())
        with tempfile.TemporaryDirectory(prefix="native provider ") as folder:
            binary = Path(folder) / "fixture"
            binary.touch()
            self.assertEqual(
                shlex.split(
                    runner.service_fixture_command(case, "native {name}", binary)
                ),
                [str(binary), "crash"],
            )
            self.assertIsNone(runner.service_fixture_command(case, None, binary))
            wrong = json.loads(json.dumps(case))
            wrong["fixtures"]["hosted/dashboard-media.py"] += "# changed provider\n"
            with self.assertRaisesRegex(ValueError, "differs"):
                runner.service_fixture_command(wrong, "native {name}", binary)
            with self.assertRaisesRegex(ValueError, "Build/select"):
                runner.service_fixture_command(case, "native {name}", None)
            wrong = json.loads(json.dumps(case))
            wrong["native_service_fixture"] = "unregistered"
            with self.assertRaisesRegex(ValueError, "Unsupported"):
                runner.service_fixture_command(wrong, "native {name}", binary)

    def provenance_fixture(self):
        folder = HERE / "nacre_shell"
        case = json.loads((folder / "case-native-image.json").read_text())
        expected = json.loads((folder / "expected-native-image.json").read_text())
        current = json.loads((runner.ROOT / "dependencies.lock.json").read_text())[
            "orient"
        ]["engine"]
        actual = copy.deepcopy(expected)
        actual["stdout"]["engine"] = current
        actual["stdout"]["source"]["engine"] = current
        return case, expected, actual, current

    def test_provenance_accepts_only_declared_fields_without_mutating_reference(self):
        case, expected, actual, current = self.provenance_fixture()
        original = copy.deepcopy(expected)
        with patch.object(runner, "pinned_engine_identity", return_value=current):
            equivalent = runner.engine_provenance_reference(expected, actual, case)
            self.assertEqual(equivalent, actual)
            actual["stdout"]["colours"]["primary"] = "010203"
            self.assertNotEqual(equivalent, actual)
            actual = copy.deepcopy(equivalent)
            actual["files"]["unexpected"] = {"mode": "0o600", "text": "changed"}
            self.assertNotEqual(equivalent, actual)
        self.assertEqual(expected, original)

    def test_provenance_rejects_recording_without_touching_frozen_outputs(self):
        reference = HERE / "nacre_shell/expected-native-image.json"
        before = reference.read_bytes()
        result = subprocess.run(
            [
                sys.executable,
                str(HERE / "run.py"),
                "--record",
                "--case",
                "nacre_shell/native-image",
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 2)
        self.assertIn("cannot be re-recorded", result.stderr)
        self.assertEqual(reference.read_bytes(), before)

    def test_provenance_rejects_wrong_observed_frozen_or_field_identity(self):
        case, expected, actual, current = self.provenance_fixture()
        with patch.object(runner, "pinned_engine_identity", return_value=current):
            wrong = copy.deepcopy(actual)
            wrong["stdout"]["source"]["engine"] = "unverified-engine"
            with self.assertRaisesRegex(ValueError, "Observed"):
                runner.engine_provenance_reference(expected, wrong, case)
            wrong = copy.deepcopy(expected)
            wrong["stdout"]["engine"] = "unverified-reference"
            with self.assertRaisesRegex(ValueError, "Frozen"):
                runner.engine_provenance_reference(wrong, actual, case)
            wrong = copy.deepcopy(case)
            wrong["engine_provenance"]["pointers"].append("/stdout/colours/primary")
            with self.assertRaisesRegex(ValueError, "Unsupported"):
                runner.engine_provenance_reference(expected, actual, wrong)

    def test_provenance_rejects_a_compiled_decoder_with_a_different_identity(self):
        runner.pinned_engine_identity.cache_clear()
        with patch.object(
            runner.subprocess,
            "run",
            return_value=type("Reply", (), {"stdout": '{"result":{}}'})(),
        ):
            with self.assertRaisesRegex(ValueError, "Compiled"):
                runner.pinned_engine_identity()
        runner.pinned_engine_identity.cache_clear()

    def test_provenance_rejects_core_changes_and_a_lock_adapter_mismatch(self):
        case, expected, actual, current = self.provenance_fixture()
        with patch.object(
            runner,
            "pinned_engine_identity",
            return_value=current.replace("55b192008cc8", "differentcore"),
        ):
            with self.assertRaisesRegex(ValueError, "Core engine changed"):
                runner.engine_provenance_reference(expected, actual, case)
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            lock = json.loads((runner.ROOT / "dependencies.lock.json").read_text())
            pin = json.loads(
                (
                    runner.ROOT / "nacre/shell-cli/src/nacre_shell/engine-pin.json"
                ).read_text()
            )
            pin["engine"] = "stale-adapter"
            target = root / "nacre/shell-cli/src/nacre_shell/engine-pin.json"
            target.parent.mkdir(parents=True)
            target.write_text(json.dumps(pin))
            (root / "dependencies.lock.json").write_text(json.dumps(lock))
            runner.pinned_engine_identity.cache_clear()
            with patch.object(runner, "ROOT", root):
                with self.assertRaisesRegex(ValueError, "lock and adapter"):
                    runner.pinned_engine_identity()
            runner.pinned_engine_identity.cache_clear()

    def test_scripted_interpreter_version_preserves_execution_and_executable(self):
        script = (
            "import json,subprocess,sys; "
            "print(json.dumps({'value':6*7,'executable':sys.executable,"
            "'version':subprocess.check_output([sys.executable,'--version']).decode().strip()}))"
        )
        result = runner.execute(
            {
                "helper": "fixture",
                "python_version": "Python 3.99.1",
                "command": shlex.join(["python3", "-c", script]),
            },
            None,
        )
        self.assertEqual(result["exit"], 0)
        self.assertEqual(
            result["stdout"],
            {"value": 42, "executable": "/usr/bin/python3", "version": "Python 3.99.1"},
        )
        self.assertEqual(result["effects"], [])

    def test_root_created_ipc_socket_belongs_to_fixture_identity(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with (
                patch.object(runner.os, "getuid", return_value=0),
                patch.object(runner.os, "chown") as chown,
            ):
                ipc = runner.IPC(root, [{"path": "runtime/fixture.sock"}])
            try:
                chown.assert_called_once_with(root / "runtime/fixture.sock", 1000, 1000)
                self.assertEqual(
                    (root / "runtime/fixture.sock").stat().st_mode & 0o777, 0o600
                )
            finally:
                ipc.close()

    def test_fixture_cannot_write_outside_root(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaisesRegex(ValueError, "escapes"):
                runner.fixtures(Path(directory), {"../outside": "bad"})

    def test_unavailable_sandbox_refuses_host_execution(self):
        with patch.object(runner.shutil, "which", return_value=None):
            with self.assertRaisesRegex(RuntimeError, "live-host fallback"):
                runner.execute({"helper": "launcher-preferences"}, None)

    def test_sandbox_start_failure_surfaces_stderr(self):
        with patch.object(runner.subprocess, "Popen") as spawn:
            process = spawn.return_value.__enter__.return_value
            process.returncode = 1
            process.communicate.return_value = (
                "",
                "bwrap: Creating new namespace failed: Operation not permitted\n",
            )
            with self.assertRaisesRegex(
                RuntimeError, "Parity sandbox.*Operation not permitted"
            ):
                runner.execute({"helper": "fixture"}, "python3 -c pass")

    def test_clean_environment_and_filesystem_observation(self):
        script = (
            "import json,os,pathlib; "
            "p=pathlib.Path.home()/'created';p.write_bytes(b'public');p.chmod(0o640);"
            "print(json.dumps({'home':str(pathlib.Path.home()),"
            "'secret_inherited':'PARITY_TEST_SECRET' in os.environ,"
            "'proc':list(pathlib.Path('/proc').iterdir())}))"
        )
        with patch.dict(os.environ, {"PARITY_TEST_SECRET": "test-only-sentinel"}):
            result = runner.execute(
                {"helper": "fixture"}, shlex.join(["python3", "-c", script])
            )
        self.assertEqual(
            result["stdout"],
            {"home": "/case/home", "secret_inherited": False, "proc": []},
        )
        self.assertEqual(
            result["files"], {"home/created": {"mode": "0o640", "text": "public"}}
        )

    def test_changed_output_and_missing_writes_fail_cli(self):
        command = shlex.join(["python3", "-c", "print('{}')"])
        result = subprocess.run(
            [
                sys.executable,
                str(HERE / "run.py"),
                "--case",
                "launcher-preferences/initialize",
                "--command",
                command,
            ],
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn("FAIL", result.stderr)
        self.assertIn("files", result.stderr)

    def test_partial_packet_timeout_and_buffered_packets(self):
        with subprocess.Popen(
            [
                sys.executable,
                "-c",
                "import sys,time;sys.stdout.write('{');sys.stdout.flush();time.sleep(5)",
            ],
            stdout=subprocess.PIPE,
        ) as process:
            try:
                with self.assertRaisesRegex(RuntimeError, "timed out"):
                    runner.read_packet(process, timeout=0.05)
            finally:
                process.kill()
                process.wait()
        with subprocess.Popen(
            [sys.executable, "-c", "print('{}');print('{\"id\":2}')"],
            stdout=subprocess.PIPE,
        ) as process:
            self.assertEqual(runner.read_packet(process), {})
            self.assertEqual(runner.read_packet(process), {"id": 2})
            process.wait()

    def test_normalization_preserves_other_fields_and_bytes(self):
        result = {
            "stdout": {"checked": 123, "count": 5},
            "files": {"text": '{"time": 123, "count":5}'},
        }
        runner.normalize(
            result,
            [
                {
                    "kind": "timestamp",
                    "pointer": "/stdout/checked",
                    "reason": "sampling time",
                },
                {
                    "kind": "timestamp",
                    "pointer": "/files/text",
                    "json_pointer": "/time",
                    "reason": "persisted sampling time",
                },
            ],
        )
        self.assertEqual(result["stdout"], {"checked": "<timestamp>", "count": 5})
        self.assertEqual(result["files"]["text"], '{"time": "<timestamp>", "count":5}')
        with self.assertRaises(ValueError):
            runner.normalize(
                result,
                [{"kind": "uuid", "pointer": "/stdout/count", "reason": "forbidden"}],
            )

    def test_direct_orient_policy_keeps_data_writes_and_other_actions_exact(self):
        call = {
            "command": "orient",
            "argv": ["--rpc", "--serve"],
            "stdin": '{"id":1,"operation":"from_image","payload":{"path":"/case/home/demo.png"}}\n',
        }
        desktop = {
            "command": "gsettings",
            "argv": ["set", "schema", "key", "value"],
            "stdin": "",
        }
        reference = {
            "stdout": {"engine": "exact-pin", "colours": {"primary": "224466"}},
            "files": {"home/state": {"text": "exact"}},
            "effects": [call, desktop],
        }
        direct = {**reference, "effects": [desktop]}
        case = {"native_engine": True}
        self.assertEqual(
            runner.orient_direct_equivalence(reference, case),
            runner.orient_direct_equivalence(direct, case),
        )
        changed = {**direct, "stdout": {"engine": "different-pin"}}
        self.assertNotEqual(
            runner.orient_direct_equivalence(reference, case),
            runner.orient_direct_equivalence(changed, case),
        )
        self.assertEqual(runner.orient_direct_equivalence(reference, {}), reference)
        for event in (
            {**call, "argv": ["--rpc", "--unexpected"]},
            {**call, "stdin": '{"operation":"publish","payload":{}}\n'},
            {**call, "stdin": "malformed\n"},
        ):
            value = {**direct, "effects": [event]}
            self.assertEqual(
                runner.orient_direct_equivalence(value, case)["effects"], [event]
            )


if __name__ == "__main__":
    unittest.main()
