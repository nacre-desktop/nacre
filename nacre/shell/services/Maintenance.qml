pragma Singleton
import QtQuick
import qs.utils
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

        worker.command = ["python3", NacrePaths.tools + "/maintenance.py", action];
        worker.running = true;
    }

    function recover(action) {
        AppLaunch.run(["python3", NacrePaths.tools + "/maintenance.py", action]);
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
                        root.message = NacreI18n.tr("ui.maintenance.message.checked-value0", "Checked {value0}", {
                            value0: new Date().toLocaleTimeString()
                        });
                    }
                } catch (e) {
                    root.message = NacreI18n.tr("ui.maintenance.message.could-not-read-maintenance-status", "Could not read maintenance status");
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
