import QtQuick
import Quickshell
import Quickshell.Io
import qs.utils

Scope {
    id: root
    property bool active: true
    property string helper: NacrePaths.helperBinary
    property var data: ({
            extensions: []
        })
    property string message: ""
    property bool refreshWanted: false
    readonly property bool busy: action.running
    signal completed(string operation, bool success)
    function refresh() {
        if (!active)
            return;
        if (loader.running)
            refreshWanted = true;
        else
            loader.running = true;
    }
    function work(operation, value = "", payload = {}) {
        if (action.running) {
            message = NacreI18n.tr("software.library.finishing", "Another software action is finishing");
            return false;
        }
        message = "";
        action.operation = operation;
        action.value = value;
        action.payload = payload;
        action.running = true;
        return true;
    }
    Process {
        id: loader
        objectName: "softwareLibraryLoad"
        command: [root.helper, "software-library", "load"]
        onExited: if (root.refreshWanted) {
            root.refreshWanted = false;
            Qt.callLater(root.refresh);
        }
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    const value = JSON.parse(text);
                    if (value.error)
                        root.message = value.error;
                    else if (JSON.stringify(root.data) !== JSON.stringify(value))
                        root.data = value;
                } catch (error) {
                    root.message = NacreI18n.tr("software.library.read-error", "Could not read the software library");
                }
            }
        }
    }
    Process {
        id: action
        objectName: "softwareLibraryAction"
        property string operation: ""
        property string value: ""
        property var payload: ({})
        command: [root.helper, "software-library", operation, value]
        stdinEnabled: true
        onStarted: {
            write(JSON.stringify(payload));
            stdinEnabled = false;
        }
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    const value = JSON.parse(text);
                    root.message = value.error || value.message || "";
                    if (!value.error && action.operation === "clone" && value.folder)
                        AppLaunch.run(["xdg-open", value.folder]);
                    root.completed(action.operation, !value.error);
                } catch (error) {
                    root.message = NacreI18n.tr("software.library.action-error", "Could not complete the software action");
                    root.completed(action.operation, false);
                }
            }
        }
        onExited: {
            stdinEnabled = true;
            root.refresh();
        }
    }
    Component.onCompleted: refresh()
    Timer {
        interval: 15000
        repeat: true
        running: root.active
        onTriggered: root.refresh()
    }
}
