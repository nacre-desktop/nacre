"""Setup discovery and typed plans against private device/tool fixtures."""

import json
import os
import pty
import select
import shutil
from pathlib import Path
import subprocess
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[2]
BINARY = Path(os.environ.get("NACRE_HELPER_BINARY", ROOT / "target/debug/nacre-helper"))


@unittest.skipUnless(BINARY.is_file(), "Candidate native helper required")
class SetupTests(unittest.TestCase):
    def test_combined_keyboard_scroll_and_touchpad_config_passes_actual_niri_parser(
        self,
    ):
        niri = (
            Path("/usr/bin/niri")
            if Path("/usr/bin/niri").is_file()
            else Path(shutil.which("niri") or "/absent-niri")
        )
        if not niri.is_file():
            self.skipTest("Native Niri validator required")
        with tempfile.TemporaryDirectory(prefix="nacre-input-parser-") as name:
            root = Path(name)
            env = dict(os.environ)
            for key, suffix in [
                ("HOME", "home"),
                ("XDG_CONFIG_HOME", "config"),
                ("XDG_STATE_HOME", "state"),
                ("XDG_DATA_HOME", "data"),
                ("XDG_CACHE_HOME", "cache"),
                ("XDG_RUNTIME_DIR", "runtime"),
            ]:
                env[key] = str(root / suffix)
            Path(env["HOME"]).mkdir()
            Path(env["XDG_RUNTIME_DIR"]).mkdir(mode=0o700)
            env.update(
                DBUS_SESSION_BUS_ADDRESS="unix:path=" + str(root / "absent"),
                DBUS_SYSTEM_BUS_ADDRESS="unix:path=" + str(root / "absent"),
            )
            for key in [
                "DISPLAY",
                "WAYLAND_DISPLAY",
                "NIRI_SOCKET",
                "HYPRLAND_INSTANCE_SIGNATURE",
            ]:
                env.pop(key, None)
            for preferences in [
                {"keyboardLayouts": "us,no", "naturalScroll": True},
                {
                    "keyboardLayouts": "us,no",
                    "naturalScroll": False,
                    "touchpadEnabled": False,
                },
            ]:
                with self.subTest(preferences=preferences):
                    render = subprocess.run(
                        [
                            str(BINARY),
                            "compositor-render",
                            "niri",
                            "render_desktop_options",
                            json.dumps(preferences),
                        ],
                        env=env,
                        capture_output=True,
                        text=True,
                        timeout=5,
                    )
                    self.assertEqual(render.returncode, 0, render.stderr)
                    path = root / "input.kdl"
                    path.write_text(json.loads(render.stdout))
                    result = subprocess.run(
                        [str(niri), "validate", "-c", str(path)],
                        env=env,
                        capture_output=True,
                        text=True,
                        timeout=10,
                    )
                    self.assertEqual(
                        result.returncode, 0, result.stdout + result.stderr
                    )

    def test_wifi_qr_never_prints_plaintext_credentials_or_uses_secret_arguments(self):
        if Path("/proc/self/uid_map").read_text().split() != ["0", "0", "4294967295"]:
            self.skipTest(
                "Mapped-host dispatch is covered separately; QR child runs on the normal host"
            )
        with tempfile.TemporaryDirectory(prefix="nacre-qr-") as name:
            root = Path(name)
            commands = root / "bin"
            commands.mkdir()
            payload = root / "qr-payload"
            arguments = root / "arguments.jsonl"
            uuid = "01234567-89ab-cdef-0123-456789abcdef"
            script = (
                "#!/usr/bin/python3\nimport os,sys,json\n"
                "name=os.path.basename(sys.argv[0]);args=sys.argv[1:]\n"
                "if name=='systemctl':print('loaded')\n"
                "elif name=='fwupdmgr':print(json.dumps({'Devices':[]}))\n"
                f"elif name=='nmcli':print('{uuid}:802-11-wireless:wlan-fixture')\n"
                "elif name=='busctl':print(json.dumps({'data':[[]]}))\n"
                "elif name=='fido2-token':pass\n"
                "elif name=='pkexec':\n"
                f" with open({str(arguments)!r},'a') as f:f.write(json.dumps(args)+'\\n')\n"
                " key=args[3];print({'802-11-wireless.ssid':'Fixture;WiFi','802-11-wireless-security.key-mgmt':'wpa-psk','802-11-wireless-security.psk':'fixture-secret'}[key])\n"
                "elif name=='qrencode':\n"
                f" open({str(payload)!r},'w').write(sys.stdin.read())\n"
                " print('QR-fixture-rendered')\n"
            )
            for tool in [
                "systemctl",
                "fwupdmgr",
                "nmcli",
                "busctl",
                "fido2-token",
                "pkexec",
                "qrencode",
                "kitty",
            ]:
                p = commands / tool
                p.write_text(script)
                p.chmod(0o755)
            env = {**os.environ, "HOME": str(root), "PATH": str(commands) + ":/usr/bin"}
            for key in [
                "DISPLAY",
                "WAYLAND_DISPLAY",
                "NIRI_SOCKET",
                "HYPRLAND_INSTANCE_SIGNATURE",
            ]:
                env.pop(key, None)
            for key, suffix in [
                ("XDG_CONFIG_HOME", "config"),
                ("XDG_STATE_HOME", "state"),
                ("XDG_DATA_HOME", "data"),
                ("XDG_CACHE_HOME", "cache"),
                ("XDG_RUNTIME_DIR", "runtime"),
            ]:
                env[key] = str(root / suffix)
            env.update(
                DBUS_SESSION_BUS_ADDRESS="unix:path=" + str(root / "absent"),
                DBUS_SYSTEM_BUS_ADDRESS="unix:path=" + str(root / "absent"),
            )
            master, slave = pty.openpty()
            child = subprocess.Popen(
                [str(BINARY), "software-actions", "run", "wifi-qr"],
                env=env,
                stdin=slave,
                stdout=slave,
                stderr=slave,
            )
            os.close(slave)
            output = b""
            try:
                deadline = time.monotonic() + 10
                while (
                    b"Type yes to run these commands:" not in output
                    and time.monotonic() < deadline
                ):
                    if select.select([master], [], [], 0.2)[0]:
                        output += os.read(master, 8192)
                self.assertIn(b"Type yes to run these commands:", output)
                self.assertFalse(payload.exists())
                os.write(master, b"yes\n\n")
                self.assertEqual(child.wait(timeout=5), 0)
                while select.select([master], [], [], 0.05)[0]:
                    try:
                        output += os.read(master, 8192)
                    except OSError:
                        break
                self.assertIn("S:Fixture\\;WiFi;", payload.read_text())
                self.assertIn("P:fixture-secret;", payload.read_text())
                self.assertNotIn(b"fixture-secret", output)
                self.assertNotIn("fixture-secret", arguments.read_text())
            finally:
                if child.poll() is None:
                    child.terminate()
                    child.wait(timeout=3)
                os.close(master)

    def test_reader_keys_connection_identity_and_dns_validation(self):
        with tempfile.TemporaryDirectory(prefix="nacre-setup-") as name:
            root = Path(name)
            commands = root / "bin"
            commands.mkdir()
            log = root / "mutations.jsonl"
            uuid = "01234567-89ab-cdef-0123-456789abcdef"
            script = (
                "#!/usr/bin/python3\nimport os,sys,json\n"
                "name=os.path.basename(sys.argv[0]);args=sys.argv[1:]\n"
                "if name=='systemctl' and 'show' in args:print('loaded')\n"
                "elif name=='fwupdmgr':print(json.dumps({'Devices':[]}))\n"
                "elif name=='busctl':print(json.dumps({'type':'ao','data':[['/net/reactivated/Fprint/Device/0']] if os.getenv('READER')=='yes' else [[]]}))\n"
                "elif name=='fido2-token' and args==['-L']:print('/dev/hidraw3: fixture key')\n"
                f"elif name=='nmcli' and 'show' in args:print('{uuid}:802-11-wireless:wlan-fixture')\n"
                "else:\n"
                f" with open({str(log)!r},'a') as f:f.write(json.dumps([name,*args])+'\\n')\n"
            )
            for tool in [
                "systemctl",
                "fwupdmgr",
                "busctl",
                "fido2-token",
                "fprintd-enroll",
                "nmcli",
                "kitty",
                "pkexec",
                "qrencode",
            ]:
                p = commands / tool
                p.write_text(script)
                p.chmod(0o755)
            env = {
                **os.environ,
                "HOME": str(root),
                "PATH": str(commands) + ":/usr/bin",
                "READER": "no",
            }
            for key, suffix in [
                ("XDG_CONFIG_HOME", "config"),
                ("XDG_STATE_HOME", "state"),
                ("XDG_DATA_HOME", "data"),
                ("XDG_CACHE_HOME", "cache"),
                ("XDG_RUNTIME_DIR", "runtime"),
            ]:
                env[key] = str(root / suffix)
            env.update(
                DBUS_SESSION_BUS_ADDRESS="unix:path=" + str(root / "absent"),
                DBUS_SYSTEM_BUS_ADDRESS="unix:path=" + str(root / "absent"),
            )
            for key in [
                "NIRI_SOCKET",
                "HYPRLAND_INSTANCE_SIGNATURE",
                "DISPLAY",
                "WAYLAND_DISPLAY",
            ]:
                env.pop(key, None)

            def cli(*args):
                return subprocess.run(
                    [str(BINARY), "software-actions", *args],
                    env=env,
                    capture_output=True,
                    text=True,
                    timeout=10,
                )

            rows = json.loads(cli("load").stdout)["setup"]
            self.assertFalse(any(r["id"] == "fingerprint-enroll" for r in rows))
            env["READER"] = "yes"
            self.assertTrue(
                any(
                    r["id"] == "fingerprint-enroll"
                    for r in json.loads(cli("load").stdout)["setup"]
                )
            )
            self.assertFalse(log.exists())
            result = cli(
                "plan",
                "dns-custom",
                json.dumps(
                    {"connection": uuid, "servers": "1.1.1.1,2001:4860:4860::8888"}
                ),
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            plan = json.loads(result.stdout)
            self.assertTrue(plan["steps"][0]["root"])
            self.assertIn("1.1.1.1", plan["steps"][0]["arguments"])
            self.assertNotEqual(
                cli(
                    "plan",
                    "dns-custom",
                    json.dumps({"connection": uuid, "servers": "$(bad)"}),
                ).returncode,
                0,
            )
            self.assertNotEqual(
                cli(
                    "plan", "dns-google", json.dumps({"connection": "foreign"})
                ).returncode,
                0,
            )
            self.assertNotEqual(
                cli(
                    "plan", "fido-pin", json.dumps({"key": "/dev/hidraw999"})
                ).returncode,
                0,
            )
            key = json.loads(
                cli("plan", "fido-pin", json.dumps({"key": "/dev/hidraw3"})).stdout
            )
            self.assertEqual(
                key["steps"][0]["arguments"], ["fido2-token", "-S", "/dev/hidraw3"]
            )
            self.assertFalse(log.exists())
