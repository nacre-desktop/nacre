"""Exercise the real native CLI with private roots; never contact the desktop bus."""

import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


class LauncherWorkflowCLITests(unittest.TestCase):
    def setUp(self):
        candidate = os.environ.get("NACRE_HELPER_BINARY")
        if not candidate or not Path(candidate).is_file():
            self.skipTest("Native helper build is unavailable")
        self.helper = candidate
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.home = Path(self.directory.name)
        runtime = self.home / "runtime"
        runtime.mkdir(mode=0o700)
        self.environment = {
            **os.environ,
            "HOME": str(self.home),
            "XDG_CONFIG_HOME": str(self.home / ".config"),
            "XDG_STATE_HOME": str(self.home / ".state"),
            "XDG_CACHE_HOME": str(self.home / ".cache"),
            "XDG_RUNTIME_DIR": str(runtime),
            "DBUS_SESSION_BUS_ADDRESS": "unix:path=" + str(runtime / "absent-bus"),
            "DBUS_SYSTEM_BUS_ADDRESS": "unix:path="
            + str(runtime / "absent-system-bus"),
        }

    def call(self, *arguments, data=None, success=True):
        result = subprocess.run(
            [self.helper, *arguments],
            input=json.dumps(data) if data is not None else "",
            capture_output=True,
            text=True,
            env=self.environment,
            timeout=10,
        )
        self.assertEqual(result.returncode == 0, success, result.stderr + result.stdout)
        return json.loads(result.stdout)

    def test_saved_arguments_are_literal_and_registry_projects_are_file_roots(self):
        project = self.home / "outside-projects"
        project.mkdir()
        document = project / "report.txt"
        document.write_text("local preview")
        config = self.home / ".config/nacre"
        (config / "ai").mkdir(parents=True)
        (config / "ai/projects.json").write_text(
            json.dumps(
                {"projects": [{"id": "demo", "label": "Demo", "path": str(project)}]}
            )
        )
        binary = self.home / "bin"
        binary.mkdir()
        # The fixture supplies fd's bytes, not the behavior of the helper being tested.
        finder = binary / "fd"
        finder.write_text(
            "#!/usr/bin/python3\nimport json,os,sys\n"
            "from pathlib import Path\n"
            "Path(os.environ['HOME'],'fd-arguments.json').write_text(json.dumps(sys.argv[1:]))\n"
            "sys.stdout.buffer.write(str(Path(os.environ['HOME'],'outside-projects/report.txt')).encode()+b'\\0')\n"
        )
        finder.chmod(0o700)
        self.environment["PATH"] = str(binary) + os.pathsep + self.environment["PATH"]
        result = self.call("menu-backend", "files", "report")
        self.assertEqual(result["rows"][0]["path"], str(document))
        self.assertIn(
            str(project), json.loads((self.home / "fd-arguments.json").read_text())
        )
        self.assertEqual(
            self.call("launcher-workflows", "preview", str(document))["text"],
            "local preview",
        )
        self.call(
            "launcher-workflows",
            "save",
            "action",
            data={
                "label": "Literal action",
                "executable": "printf",
                "arguments": "$(touch SHOULD_NOT_EXIST)\none argument with spaces",
                "directory": str(project),
                "icon": "terminal",
            },
        )
        saved = json.loads((config / "launcher-workflows.json").read_text())
        self.assertEqual(
            saved["entries"][0]["arguments"],
            ["printf", "$(touch SHOULD_NOT_EXIST)", "one argument with spaces"],
        )
        self.assertFalse((project / "SHOULD_NOT_EXIST").exists())
        before = (config / "launcher-workflows.json").read_bytes()
        self.call(
            "launcher-workflows",
            "save",
            "bookmark",
            data={"label": "Bad", "url": "javascript:bad"},
            success=False,
        )
        self.assertEqual((config / "launcher-workflows.json").read_bytes(), before)

    def test_preview_rejects_symlink_escape_and_encodes_image_urls(self):
        documents = self.home / "Documents"
        documents.mkdir()
        outside = self.home / "outside.txt"
        outside.write_text("outside")
        (documents / "escape.txt").symlink_to(outside)
        result = self.call(
            "launcher-workflows",
            "preview",
            str(documents / "escape.txt"),
            success=False,
        )
        self.assertIn("outside", result["error"])
        image = documents / "image # one.png"
        image.write_bytes(b"fixture image bytes")
        result = self.call("launcher-workflows", "preview", str(image))
        self.assertTrue(result["image"].endswith("image%20%23%20one.png"))
