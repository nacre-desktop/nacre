pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

Singleton {
    id: root
    property var data: ({})
    property string error: ""
    property bool refreshQueued: false
    property string action: "state"
    property string value: ""
    readonly property string tool: Quickshell.env("HOME") + "/.local/share/nacre/shell/tools/control-tools.py"
    readonly property bool busy: worker.running
    readonly property bool visibleControls: Object.values(NacrePanelState.screens).some(view => view.osd)
    function refresh() {
        if (busy) {
            refreshQueued = true;
            return;
        }
        action = "state";
        value = "";
        worker.running = true;
    }
    function toggleNightLight() {
        if (busy || !data.nightLightSupported || data.nightLightExternal)
            return;
        error = "";
        action = "night-light";
        value = data.nightLightEnabled ? "off" : "on";
        worker.running = true;
    }
    function capture(kind) {
        if (!["screenshot", "color-picker"].includes(kind))
            return;
        NacrePanelState.close();
        captureDelay.kind = kind;
        captureDelay.restart();
    }
    onVisibleControlsChanged: if (visibleControls)
        refresh()
    Component.onCompleted: refresh()
    FileView {
        path: Quickshell.env("HOME") + "/.local/state/nacre/control-tools.json"
        watchChanges: true
        printErrors: false
        onFileChanged: {
            reload();
            root.refresh();
        }
    }
    Process {
        id: worker
        objectName: "controlToolsWorker"
        command: ["python3", root.tool, root.action, ...(root.value ? [root.value] : [])]
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    const result = JSON.parse(text);
                    if (result.error)
                        root.error = result.error;
                    else {
                        root.data = result;
                        root.error = "";
                    }
                } catch (failure) {
                    root.error = "Could not read control tools";
                }
            }
        }
        onExited: {
            if (root.refreshQueued) {
                root.refreshQueued = false;
                followup.start();
            }
        }
    }
    Timer {
        id: followup
        interval: 1
        onTriggered: root.refresh()
    }
    Timer {
        id: captureDelay
        property string kind: ""
        interval: 320
        onTriggered: AppLaunch.run(["python3", root.tool, kind])
    }
}
