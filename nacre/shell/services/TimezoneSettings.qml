pragma Singleton
import QtQuick
import qs.utils
import Quickshell
import Quickshell.Io

Singleton {
    id: root

    property var status: ({})
    property string message: ""
    property var zones: ["Etc/UTC"]
    readonly property Process zoneReader: Process {
        running: true
        command: ["timedatectl", "list-timezones"]
        stdout: StdioCollector {
            onStreamFinished: {
                const values = text.trim().split("\n").filter(v => v && !v.startsWith("."));
                if (values.length)
                    root.zones = values;
            }
        }
    }
    readonly property bool busy: action.running
    readonly property string helper: NacrePaths.tools + "/timezone.py"

    function refresh() {
        if (!reader.running)
            reader.running = true;
    }

    function change(kind, value) {
        if (busy)
            return;

        message = NacreI18n.tr("ui.timezone-settings.message.waiting-for-administrator-authentication", "Waiting for administrator authentication…");
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
                    root.message = NacreI18n.tr("ui.timezone-settings.message.could-not-read-time-settings", "Could not read time settings");
                }
            }
        }
    }

    Process {
        id: action

        onExited: code => {
            if (code !== 0)
                root.message = NacreI18n.tr("ui.timezone-settings.message.time-settings-were-not-changed-administrator-authentica", "Time settings were not changed. Administrator authentication is required.");

            root.refresh();
        }

        stdout: SplitParser {
            splitMarker: ""
            onRead: line => {
                try {
                    const data = JSON.parse(line);
                    root.message = data.error ?? NacreI18n.tr("ui.timezone-settings.message.time-settings-updated", "Time settings updated");
                } catch (e) {
                    root.message = NacreI18n.tr("ui.timezone-settings.message.could-not-update-time-settings", "Could not update time settings");
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
