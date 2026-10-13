"""Real Quickshell/Python event and action roundtrip on a private fake socket.

No compositor, notification server or session is started. The only IPC endpoint
is the temporary fixture socket; the QPA offscreen platform creates no windows.
"""

import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import tempfile
import threading

import unittest


def run_fixture_smoke(repo, binary):
    fixture = json.loads(
        (repo / "nacre/shell-tools/tests/fixtures/niri-event-stream.json").read_text()
    )
    windows = fixture["events"][1]["WindowsChanged"]["windows"]
    workspaces = fixture["events"][0]["WorkspacesChanged"]["workspaces"]
    requests = []
    streams = []
    lock = threading.Lock()
    stop = threading.Event()

    def emit(event):
        data = (json.dumps(event) + "\n").encode()
        for stream in list(streams):
            try:
                stream.sendall(data)
            except OSError:
                streams.remove(stream)

    def handle(connection):
        try:
            with connection.makefile("rb") as reader:
                request = json.loads(reader.readline())
                with lock:
                    requests.append(request)
                    if request == "EventStream":
                        connection.sendall(b'{"Ok":"Handled"}\n')
                        for event in fixture["events"][:2]:
                            connection.sendall((json.dumps(event) + "\n").encode())
                        streams.append(connection)
                    elif request == "NacreOverviewHoverStream":
                        connection.sendall(b'{"Err":"unknown request"}\n')
                    elif request == "Outputs":
                        connection.sendall(
                            (
                                json.dumps({"Ok": {"Outputs": fixture["outputs"]}})
                                + "\n"
                            ).encode()
                        )
                    elif request == "Workspaces":
                        connection.sendall(
                            (
                                json.dumps({"Ok": {"Workspaces": workspaces}}) + "\n"
                            ).encode()
                        )
                    elif request == "Windows":
                        connection.sendall(
                            (json.dumps({"Ok": {"Windows": windows}}) + "\n").encode()
                        )
                    elif isinstance(request, dict) and "Action" in request:
                        name, values = next(iter(request["Action"].items()))
                        if name == "FocusWindow":
                            for window in windows:
                                window["is_focused"] = window["id"] == values["id"]
                            emit({"WindowFocusChanged": {"id": values["id"]}})
                        elif name == "CloseWindow":
                            windows[:] = [
                                window
                                for window in windows
                                if window["id"] != values["id"]
                            ]
                            emit({"WindowClosed": {"id": values["id"]}})
                        connection.sendall(b'{"Ok":"Handled"}\n')
                    else:
                        raise RuntimeError("unexpected request")
                if request == "EventStream":
                    stop.wait(20)
        finally:
            connection.close()

    with tempfile.TemporaryDirectory(prefix="nacre-niri-qsc-") as directory:
        root = Path(directory)
        services = root / "services"
        backend = services / "compositor"
        backend.mkdir(parents=True)
        shutil.copy2(
            repo / "nacre/shell/services/NacreCompositor.qml",
            services / "NacreCompositor.qml",
        )
        (services / "qmldir").write_text(
            "singleton NacreCompositor 1.0 NacreCompositor.qml\n"
        )
        for path in (repo / "nacre/shell/services/compositor").iterdir():
            if path.is_file():
                shutil.copy2(path, backend / path.name)
        probe = root / "shell.qml"
        probe.write_text("""import QtQuick
    import Quickshell
    import "services" as Services
    Scope {
     property int stage:0
     Timer {interval:20;repeat:true;running:true;onTriggered:{
      const facade=Services.NacreCompositor
      if(stage===0 && facade.windows.length===3){
       if(facade.backendName!=="niri" || facade.activeWindow.windowId!=="niri-window:9007199254740995" || facade.activeWsId!==1)throw new Error("initial state mismatch")
       if(!facade.focusWindow("niri-window:1"))throw new Error("focus rejected")
       stage=1
      }else if(stage===1 && facade.activeWindow?.windowId==="niri-window:1"){
       if(!facade.closeWindow("niri-window:3"))throw new Error("close rejected")
       stage=2
      }else if(stage===2 && facade.windows.length===2){
       console.log("NIRI_SOURCE_RUNTIME_OK")
       Qt.quit()
      }
     }}
     Timer {interval:12000;running:true;onTriggered:{console.error("NIRI_RUNTIME_TIMEOUT: "+Services.NacreCompositor.error);Qt.quit()}}
    }
    """)
        listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        path = root / "niri.sock"
        listener.bind(str(path))
        listener.listen()
        listener.settimeout(0.1)

        def serve():
            while not stop.is_set():
                try:
                    connection, _ = listener.accept()
                except TimeoutError:
                    continue
                threading.Thread(target=handle, args=(connection,), daemon=True).start()

        server = threading.Thread(target=serve, daemon=True)
        server.start()
        environment = {
            **os.environ,
            "QT_QPA_PLATFORM": "offscreen",
            "QT_LOGGING_RULES": "qml.debug=true",
            "NACRE_COMPOSITOR": "niri",
            "NIRI_SOCKET": str(path),
            "NACRE_COMPOSITOR_TOOLS": str(repo / "nacre/shell-tools"),
        }
        try:
            result = subprocess.run(
                [binary, "-p", str(probe), "--no-duplicate"],
                env=environment,
                capture_output=True,
                text=True,
                timeout=18,
            )
            output = result.stdout + result.stderr
            if "NIRI_SOURCE_RUNTIME_OK" not in output:
                raise AssertionError(output[-5500:])
            actions = [
                request["Action"]
                for request in requests
                if isinstance(request, dict) and "Action" in request
            ]
            assert actions == [
                {"FocusWindow": {"id": 1}},
                {"CloseWindow": {"id": 3}},
            ], actions
        finally:
            stop.set()
            server.join(timeout=1)
            listener.close()


class NiriRuntimeTests(unittest.TestCase):
    @unittest.skipUnless(shutil.which("quickshell"), "Native Quickshell unavailable")
    def test_real_helper_stream_and_typed_actions_never_use_a_live_session(self):
        run_fixture_smoke(
            Path(__file__).resolve().parents[3], shutil.which("quickshell")
        )


if __name__ == "__main__":
    unittest.main()
