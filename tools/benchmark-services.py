#!/usr/bin/env python3
"""Time the production dashboard request owner, without CLI IPC/polling overhead."""

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

from performance import summarize


def benchmark(services, tools, count):
    with tempfile.TemporaryDirectory(prefix="nacre-owner-") as directory:
        root = Path(directory)
        home = root / "home"
        target = root / "services"
        target.mkdir()
        installed = home / ".local/share/nacre/shell/tools"
        installed.parent.mkdir(parents=True)
        shutil.copytree(
            tools, installed, ignore=shutil.ignore_patterns("tests", "__pycache__")
        )
        for name in ("NacreDashboardState", "NacreHelpers", "NacreHelperTask"):
            source = services / (name + ".qml")
            if source.exists():
                shutil.copy2(source, target)
        for name, body in {
            "DesktopSettings": "property var data:({dnd:false});function set(key,value){}",
            "NacrePanelState": "property bool hidden:false;property var providerViews:[]",
        }.items():
            (target / (name + ".qml")).write_text(
                "pragma Singleton\nimport QtQuick\nQtObject{" + body + "}\n"
            )
        (root / "shell.qml").write_text("""import QtQuick
import Quickshell
import qs.services
ShellRoot {
 id:root
 readonly property var model:NacreDashboardState
 property var values:[]
 property bool awaiting:false
 property real started:0
 function request(){awaiting=true;started=Date.now();model.request("read");}
 Timer{id:warm;interval:500;running:true;onTriggered:root.request()}
 Timer{id:next;interval:1;onTriggered:root.request()}
 Connections {
  target:root.model
  function onFocusStateChanged(){
   if(!root.awaiting)return;
   root.values=[...root.values,Date.now()-root.started];root.awaiting=false;
   if(root.values.length===Number(Quickshell.env("NACRE_OWNER_COUNT"))){console.log("OWNER_RESULTS "+JSON.stringify(root.values));Qt.quit();}
   else next.restart();
  }
 }
 Timer{interval:20000;running:true;onTriggered:{console.error("OWNER_FAILED "+root.model.message);Qt.quit();}}
}
""")
        environment = {
            **os.environ,
            "HOME": str(home),
            "XDG_CONFIG_HOME": str(home / ".config"),
            "XDG_CACHE_HOME": str(home / ".cache"),
            "XDG_STATE_HOME": str(home / ".local/state"),
            "QT_QPA_PLATFORM": "offscreen",
            "PATH": str(Path(sys.executable).parent) + os.pathsep + os.environ["PATH"],
            "NACRE_OWNER_COUNT": str(count),
        }
        environment.pop("NACRE_DASHBOARD_TOOLS", None)
        result = subprocess.run(
            ["quickshell", "-p", str(root), "-n"],
            env=environment,
            capture_output=True,
            text=True,
            timeout=25,
            check=True,
        )
        lines = [
            line.split("OWNER_RESULTS ", 1)[1]
            for line in result.stdout.splitlines()
            if "OWNER_RESULTS " in line
        ]
        if len(lines) != 1 or "OWNER_FAILED" in result.stdout + result.stderr:
            raise ValueError(
                "Dashboard owner benchmark failed: " + result.stdout + result.stderr
            )
        values = json.loads(lines[0])
        if len(values) != count or any(value < 0 for value in values):
            raise ValueError(
                "Invalid request timing; repeat after a wall-clock adjustment"
            )
        return {
            "boundary": "production QML request(read) to focus snapshot publication; no rendered window",
            "clock": "QML Date.now, wall-clock milliseconds, 1 ms resolution",
            "requests": count,
            "ms": summarize(values),
            "runs_ms": values,
        }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--services",
        type=Path,
        default=Path(__file__).resolve().parents[1] / "nacre/shell/services",
    )
    parser.add_argument(
        "--tools",
        type=Path,
        default=Path(__file__).resolve().parents[1] / "nacre/shell-tools",
    )
    parser.add_argument("--count", type=int, default=100)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if not 1 <= args.count <= 200:
        parser.error("count must be in 1..200")
    if not shutil.which("quickshell"):
        parser.error("Quickshell is required")
    args.output.write_text(
        json.dumps(benchmark(args.services, args.tools, args.count), indent=2) + "\n"
    )
    print(args.output)


if __name__ == "__main__":
    main()
