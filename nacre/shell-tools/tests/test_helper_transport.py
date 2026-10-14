"""Actual Quickshell transport survives worker death with no replay of pending jobs."""

import os
from pathlib import Path
import selectors
import shutil
import signal
import subprocess
import tempfile
import time
import unittest

TOOLS = Path(__file__).parents[1]
SERVICES = TOOLS.parent / "shell/services"


class HelperTransportTests(unittest.TestCase):
    def test_renderer_worker_uses_central_native_helper(self):
        source = (SERVICES / "NacreHelpers.qml").read_text()
        self.assertIn('command: [NacrePaths.helperBinary, "serve"]', source)
        self.assertNotIn("helper-service.py", source)
        self.assertNotIn("palettePython", source)

    def test_native_failure_fails_both_jobs_and_recovers_without_replay(self):
        for mode in ("kill", "malformed", "timeout"):
            with self.subTest(mode=mode):
                self.run_native_failure(mode)

    def run_native_failure(self, mode):
        binary = shutil.which("quickshell")
        if not binary:
            self.skipTest("Quickshell unavailable")
        with tempfile.TemporaryDirectory(prefix="nacre-rpc-") as directory:
            root = Path(directory)
            home = root / "home"
            tools = home / ".local/share/nacre/shell/tools"
            tools.mkdir(parents=True)
            services = root / "services"
            services.mkdir()
            utils = root / "utils"
            utils.mkdir()
            shutil.copy2(
                SERVICES.parent / "utils/NacrePaths.qml", utils / "NacrePaths.qml"
            )
            for name in ("NacreHelpers", "NacreHelperTask"):
                shutil.copy2(SERVICES / (name + ".qml"), services)
            fixture = Path(
                os.environ.get("NACRE_SERVICE_FIXTURE")
                or TOOLS.parents[1] / "target/debug/examples/service-fixture"
            )
            self.assertTrue(
                fixture.is_file(), "Build the developer native service fixture"
            )
            shutil.copy2(fixture, tools / "nacre-helper")
            (root / "shell.qml").write_text("""import QtQuick
import Quickshell
import qs.services
ShellRoot {
 id:root
 property string tools:Quickshell.env("HOME")+"/.local/share/nacre/shell/tools/"
 property int failed:0
 property bool announced:false
 function fail(message){console.error("NATIVE_FAIL "+message);Qt.quit();}
 function rejected(code){if(code!==1){fail("pending job succeeded after death");return;}failed++;if(failed===2)recovered.running=true;}
 NacreHelperTask {
  id:initial;command:[root.tools+"nacre-helper","launcher-preferences","load"];running:true
  onOutput:text=>{const value=JSON.parse(text);if(!Array.isArray(value.favorites))root.fail("initial reply");}
  onExited:code=>{if(code!==0)root.fail("initial failure");else{first.running=true;second.running=true;}}
 }
 NacreHelperTask {id:first;command:[root.tools+"nacre-helper","dashboard-data","wait"];onExited:code=>root.rejected(code)}
 NacreHelperTask {id:second;command:[root.tools+"nacre-helper","dashboard-data","wait"];onExited:code=>root.rejected(code)}
 Timer {interval:150;running:NacreHelpers.count===2&&!root.announced;onTriggered:{root.announced=true;
    const mode=Quickshell.env("NACRE_TEST_FAILURE");
    if(mode==="malformed")NacreHelpers.receive("malformed");
    else if(mode==="timeout")for(const record of Object.values(NacreHelpers.pending))record.deadline=Date.now()-1;
    else console.log("NATIVE_KILL_WORKER");}}
 NacreHelperTask {
  id:recovered;command:[root.tools+"nacre-helper","launcher-preferences","favorite"];stdinEnabled:true
  onStarted:write(JSON.stringify({id:"fixture.desktop",enabled:true}))
  onOutput:text=>{const value=JSON.parse(text);if(value.favorites?.[0]!=="fixture.desktop")root.fail("recovery reply");}
  onExited:code=>{if(code!==0||root.failed!==2)root.fail("recovery failed");else{console.log("HELPERS_NATIVE_PASS");Qt.quit();}}
 }
 Timer {interval:12000;running:true;onTriggered:root.fail("test timed out")}
}
""")
            marker = root / "started"
            environment = {
                **os.environ,
                "HOME": str(home),
                "XDG_CONFIG_HOME": str(home / ".config"),
                "XDG_CACHE_HOME": str(home / ".cache"),
                "XDG_STATE_HOME": str(home / ".local/state"),
                "QT_QPA_PLATFORM": "offscreen",
                "QT_LOGGING_RULES": "qml.debug=true",
                "NACRE_TEST_MARKER": str(marker),
                "NACRE_TEST_FAILURE": mode,
            }
            environment.pop("NACRE_DASHBOARD_TOOLS", None)
            worker = subprocess.Popen(
                [binary, "-p", str(root), "-n"],
                env=environment,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
            )
            output = b""
            killed = False
            selector = selectors.DefaultSelector()
            selector.register(worker.stdout, selectors.EVENT_READ)
            try:
                deadline = time.monotonic() + 15
                while time.monotonic() < deadline:
                    for key, _ in selector.select(0.1):
                        data = os.read(key.fileobj.fileno(), 65536)
                        if data:
                            output += data
                        else:
                            selector.unregister(key.fileobj)
                    if (
                        b"NATIVE_KILL_WORKER" in output
                        and marker.exists()
                        and not killed
                    ):
                        children = (
                            Path(f"/proc/{worker.pid}/task/{worker.pid}/children")
                            .read_text()
                            .split()
                        )
                        targets = [
                            int(pid)
                            for pid in children
                            if b"nacre-helper"
                            in Path(f"/proc/{pid}/cmdline").read_bytes()
                        ]
                        self.assertEqual(
                            len(targets), 1, output.decode(errors="replace")
                        )
                        os.kill(targets[0], signal.SIGKILL)
                        killed = True
                    if worker.poll() is not None and not selector.get_map():
                        break
                if mode == "kill":
                    self.assertTrue(killed, output.decode(errors="replace"))
                self.assertEqual(
                    worker.wait(timeout=1), 0, output.decode(errors="replace")
                )
                self.assertIn(b"HELPERS_NATIVE_PASS", output)
                self.assertNotIn(b"NATIVE_FAIL", output)
                self.assertNotIn(b"ERROR", output)
                self.assertEqual(marker.read_text(), "started\n")
            finally:
                selector.close()
                if worker.poll() is None:
                    worker.terminate()
                    worker.wait(timeout=3)
                worker.stdout.close()
