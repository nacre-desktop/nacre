pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

Singleton {
    id: root

    property var status: ({})
    property string message: ""
    readonly property bool busy: action.running
    readonly property string helper: Quickshell.env("HOME") + "/.local/share/nacre/shell/tools/timezone.py"

    function refresh() {
        if (!reader.running)
            reader.running = true;
    }

    function change(kind, value) {
        if (busy)
            return;

        message = "Waiting for administrator authentication…";
        action.command = ["pkexec", "/usr/bin/python3", status.installed && kind !== "install" ? (status.installedProgram || "/usr/local/libexec/nacre-timezone.py") : helper, kind, value];
        action.running = true;
    }

    Process {
        id: reader

        running: true
        command: ["python3", root.helper, "state"]

        stdout: SplitParser {
            splitMarker: ""
            onRead: line => {
                try {
                    root.status = JSON.parse(line);
                } catch (e) {
                    root.message = "Could not read time settings";
                }
            }
        }
    }

    Process {
        id: action

        onExited: code => {
            if (code !== 0)
                root.message = "Time settings were not changed. Administrator authentication is required.";

            root.refresh();
        }

        stdout: SplitParser {
            splitMarker: ""
            onRead: line => {
                try {
                    const data = JSON.parse(line);
                    root.message = data.error ?? "Time settings updated";
                } catch (e) {
                    root.message = "Could not update time settings";
                }
            }
        }
    }

    Timer {
        interval: 15000
        repeat: true
        running: NacrePanelState.settingsVisible && NacrePanelState.settingsPage === "time"
        onTriggered: root.refresh()
    }
}
