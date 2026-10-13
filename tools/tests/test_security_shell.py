"""Exercise shell security boundaries with disposable homes and fake services."""

import json
import importlib.util
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class ShellPackagingTests(unittest.TestCase):
    def test_window_state_parser_dependency_is_in_the_core_package(self):
        spec = importlib.util.spec_from_file_location(
            "security_recipe", ROOT / "packaging/arch/recipe.py"
        )
        recipe = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(recipe)
        rendered = recipe.render(ROOT, "fixture-revision")
        core = rendered.split("package_nacre() {", 1)[1].split("package_nacre-ai()", 1)[
            0
        ]
        self.assertIn("'jq'", core.split("depends=", 1)[1].splitlines()[0])
        checked_in = (ROOT / "packaging/arch/PKGBUILD").read_text()
        core = checked_in.split("package_nacre() {", 1)[1].split(
            "package_nacre-ai()", 1
        )[0]
        self.assertIn("'jq'", core.split("depends=", 1)[1].splitlines()[0])


class PrivateCodexTests(unittest.TestCase):
    def test_local_and_remote_wrappers_create_private_provider_files(self):
        for wrapper in ("bin/codex", "ai/remote-codex-wrapper.sh"):
            with self.subTest(wrapper=wrapper), tempfile.TemporaryDirectory() as folder:
                home = Path(folder)
                executable = (
                    home / ".local/share/nacre/ai/codex/packages/0.159.2/bin/codex"
                )
                executable.parent.mkdir(parents=True)
                executable.write_text(
                    '#!/bin/bash\nmkdir -p "$HOME/provider"\n'
                    'printf fixture > "$HOME/provider/credential.json"\n'
                    'printf "%s\\n" "$@"\n'
                )
                executable.chmod(0o755)
                env = dict(os.environ, HOME=folder, CODEX_HOME=str(home / "account"))
                result = subprocess.run(
                    ["bash", str(ROOT / wrapper), "literal;$(false)", "two words"],
                    env=env,
                    capture_output=True,
                    text=True,
                    check=True,
                    umask=0o022,
                )
                self.assertEqual(
                    result.stdout.splitlines(), ["literal;$(false)", "two words"]
                )
                self.assertEqual((home / "provider").stat().st_mode & 0o777, 0o700)
                self.assertEqual(
                    (home / "provider/credential.json").stat().st_mode & 0o777, 0o600
                )


@unittest.skipUnless(shutil.which("jq"), "jq unavailable")
class WindowTrashSecurityTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.home = Path(self.temporary.name)
        self.cache = self.home / "cache"
        self.state = self.cache / "hypr-trash"
        self.state.mkdir(parents=True)
        self.tools = self.home / "tools"
        self.tools.mkdir()
        # Never contact the session compositor, D-Bus, notifications or timers.
        fake = self.tools / "python3"
        fake.write_text(
            "#!/usr/bin/python3\n"
            "import json, os, re, sys\n"
            "from pathlib import Path\n"
            "args = sys.argv[2:]\n"
            "if args[0] == 'valid-window-id':\n"
            "    sys.exit(0 if re.fullmatch(r'0x[0-9a-f]+', args[1]) else 1)\n"
            "if args[0] == 'window-job-token': print('fixture'); sys.exit()\n"
            "if args[0] == 'query':\n"
            "    row = dict(address='0xabc', workspace=dict(id=3, name='special:trash'))\n"
            "    if args[1] == 'active_window':\n"
            "        row['workspace'] = dict(id=3, name=os.environ['WORKSPACE_NAME'])\n"
            "    print(json.dumps([row] if args[1] == 'windows' else row)); sys.exit()\n"
            "if args[0] in ('move-window', 'focus-window', 'close-window', 'kill-window'):\n"
            "    with Path(os.environ['CALLS']).open('a') as stream:\n"
            "        stream.write(json.dumps(args) + '\\n')\n"
        )
        fake.chmod(0o755)
        for name in ("systemd-run", "notify-send"):
            stub = self.tools / name
            stub.write_text("#!/bin/sh\nexit 0\n")
            stub.chmod(0o755)
        self.calls = self.home / "calls"
        self.env = dict(
            os.environ,
            HOME=str(self.home),
            XDG_CACHE_HOME=str(self.cache),
            PATH=str(self.tools) + os.pathsep + os.defpath,
            CALLS=str(self.calls),
            WORKSPACE_NAME="name with 'quotes'; $(touch unwanted)\nsecond line",
        )

    def run_script(self, *args):
        return subprocess.run(
            ["bash", str(ROOT / "hypr/scripts/window-trash.sh"), *args],
            env=self.env,
            cwd=self.home,
            capture_output=True,
            text=True,
            timeout=5,
            umask=0o022,
        )

    def test_json_round_trip_preserves_literal_workspace_and_private_modes(self):
        result = self.run_script("trash")
        self.assertEqual(result.returncode, 0, result.stderr)
        record = self.state / "0xabc"
        self.assertEqual(
            json.loads(record.read_text())["workspace_name"], self.env["WORKSPACE_NAME"]
        )
        self.assertEqual(record.stat().st_mode & 0o777, 0o600)
        self.assertEqual(self.state.stat().st_mode & 0o777, 0o700)
        self.calls.unlink()
        result = self.run_script("restore-last")
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = [json.loads(line) for line in self.calls.read_text().splitlines()]
        self.assertEqual(calls[0], ["move-window", "3", "0xabc", "false"])
        self.assertFalse((self.home / "unwanted").exists())
        self.assertFalse(record.exists())

    def test_old_shell_and_malformed_records_are_never_executed(self):
        record = self.state / "0xabc"
        for content in (
            'touch "$HOME/unwanted"\n',
            json.dumps(
                dict(
                    address="0xabc",
                    workspace_id="3",
                    workspace_name="safe",
                    trashed_at="$(touch unwanted)",
                )
            ),
            "{}\n{}",
        ):
            with self.subTest(content=content):
                record.write_text(content)
                self.assertEqual(self.run_script("expire", "0xabc").returncode, 0)
                self.assertFalse((self.home / "unwanted").exists())
                self.assertFalse(self.calls.exists())

    def test_traversal_and_linked_records_do_not_read_or_execute_other_files(self):
        outside = self.home / "outside"
        outside.write_text('touch "$HOME/unwanted"\n')
        self.assertEqual(self.run_script("expire", "../../outside").returncode, 0)
        (self.state / "0xabc").symlink_to(outside)
        self.assertEqual(self.run_script("expire", "0xabc").returncode, 0)
        self.assertFalse((self.home / "unwanted").exists())
        self.assertEqual(outside.read_text(), 'touch "$HOME/unwanted"\n')

    def test_linked_state_directory_is_refused(self):
        self.state.rmdir()
        outside = self.home / "outside"
        outside.mkdir(mode=0o755)
        self.state.symlink_to(outside)
        self.assertNotEqual(self.run_script("status").returncode, 0)
        self.assertEqual(outside.stat().st_mode & 0o777, 0o755)
