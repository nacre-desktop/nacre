pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

Singleton {
    id: root

    property var data: ({})
    property string message: ""
    readonly property bool busy: worker.running

    function refresh() {
        if (!busy)
            request("state");
    }

    function request(action) {
        if (busy)
            return;

        worker.command = ["python3", Quickshell.env("HOME") + "/.local/share/nacre/shell/tools/maintenance.py", action];
        worker.running = true;
    }

    function recover(action) {
        AppLaunch.run(["python3", Quickshell.env("HOME") + "/.local/share/nacre/shell/tools/maintenance.py", action]);
    }

    Component.onCompleted: refresh()

    Process {
        id: worker

        stdout: SplitParser {
            splitMarker: ""
            onRead: line => {
                try {
                    const value = JSON.parse(line);
                    if (value.error) {
                        root.message = value.error;
                    } else {
                        root.data = value;
                        root.message = "Checked " + new Date().toLocaleTimeString();
                    }
                } catch (e) {
                    root.message = "Could not read maintenance status";
                }
            }
        }
    }

    Timer {
        interval: 15000
        repeat: true
        running: NacrePanelState.settingsVisible && NacrePanelState.settingsPage === "maintenance"
        onTriggered: root.refresh()
    }
}
