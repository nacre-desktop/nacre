"""The real QML publisher must use guarded native writes, including downgrades."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[3]


class StatePublisherTests(unittest.TestCase):
    def test_native_qml_migrates_backups_and_refuses_newer_state(self):
        binary = shutil.which("quickshell")
        helper = Path(
            os.environ.get("NACRE_HELPER_BINARY", ROOT / "target/debug/nacre-helper")
        )
        if not binary or not helper.is_file():
            self.skipTest("Native Quickshell/helper unavailable")
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            home = root / "home"
            tools = home / ".local/share/nacre/shell/tools"
            tools.mkdir(parents=True)
            shutil.copy2(helper, tools / "nacre-helper")
            utils = root / "utils"
            utils.mkdir()
            for name in ("NacreStateFile.qml", "NacrePaths.qml"):
                shutil.copy2(ROOT / "nacre/shell/utils" / name, utils / name)
            (utils / "qmldir").write_text(
                "singleton NacrePaths 1.0 NacrePaths.qml\nNacreStateFile 1.0 NacreStateFile.qml\n"
            )
            config = home / ".config/nacre"
            config.mkdir(parents=True)
            state = config / "control-center.json"
            for original, signal in [
                (b'{"version":1,"optionalTiles":["vpn"]}', "saved"),
                (b'{"version":99,"future":"keep"}', "saveFailed"),
            ]:
                with self.subTest(signal=signal):
                    state.write_bytes(original)
                    (root / "shell.qml").write_text("""import QtQuick
import Quickshell
import "utils"
ShellRoot {
 NacreStateFile {id:store;path:NacrePaths.config+"/control-center.json"
  onSaved:{console.log("STATE_SAVED");Qt.quit()}
  onSaveFailed:{console.log("STATE_REFUSED");Qt.quit()}
 }
 Component.onCompleted:store.saveText(JSON.stringify({version:2,tiles:["volume","vpn"]}))
 Timer {interval:5000;running:true;onTriggered:{console.log("STATE_TIMEOUT");Qt.quit()}}
}""")
                    environment = dict(
                        os.environ,
                        HOME=str(home),
                        XDG_CONFIG_HOME=str(home / ".config"),
                        XDG_STATE_HOME=str(home / ".local/state"),
                        XDG_CACHE_HOME=str(home / ".cache"),
                        XDG_RUNTIME_DIR=str(root / "runtime"),
                        QT_QPA_PLATFORM="offscreen",
                    )
                    for key in (
                        "DISPLAY",
                        "WAYLAND_DISPLAY",
                        "NIRI_SOCKET",
                        "HYPRLAND_INSTANCE_SIGNATURE",
                        "QML_IMPORT_PATH",
                        "QML2_IMPORT_PATH",
                    ):
                        environment.pop(key, None)
                    (root / "runtime").mkdir(exist_ok=True, mode=0o700)
                    try:
                        result = subprocess.run(
                            [binary, "-p", str(root / "shell.qml"), "-n"],
                            env=environment,
                            text=True,
                            capture_output=True,
                            timeout=10,
                        )
                    except subprocess.TimeoutExpired as error:
                        self.fail(str(error.stdout) + str(error.stderr))
                    output = result.stdout + result.stderr
                    self.assertEqual(result.returncode, 0, output)
                    self.assertNotIn("STATE_TIMEOUT", output)
                    self.assertIn(
                        "STATE_SAVED" if signal == "saved" else "STATE_REFUSED", output
                    )
                    if signal == "saved":
                        self.assertEqual(json.loads(state.read_bytes())["version"], 2)
                        self.assertEqual(
                            state.with_name(
                                state.name + ".pre-v1-to-v2.bak"
                            ).read_bytes(),
                            original,
                        )
                    else:
                        self.assertEqual(state.read_bytes(), original)
