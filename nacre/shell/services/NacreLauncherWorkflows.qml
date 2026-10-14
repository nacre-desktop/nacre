import QtQuick
import Quickshell
import Quickshell.Io
import qs.utils

Scope {
    id: root
    signal completed(bool success)
    property bool active: true
    property string helper: NacrePaths.helperBinary
    property var data: ({
            entries: [],
            projects: [],
            roots: [],
            captures: [],
            recording: false
        })
    property string message: ""
    property bool busy: action.running
    property double clock: Date.now() / 1000
    readonly property string elapsed: {
        const seconds = Math.max(0, Math.floor(clock - (Number(data.recordingStarted) || clock)));
        return Math.floor(seconds / 60) + ":" + String(seconds % 60).padStart(2, "0");
    }
    function refresh() {
        if (active && !loader.running)
            loader.running = true;
    }
    function work(operation, value = "", payload = {}) {
        if (operation === "record-stop") {
            if (stopWorker.running)
                return false;
            stopWorker.running = true;
            return true;
        }
        if (action.running) {
            message = NacreI18n.tr("launcher.workflows.finishing", "Another launcher action is finishing");
            return false;
        }
        message = "";
        action.operation = operation;
        action.value = value;
        action.payload = payload;
        action.running = true;
        return true;
    }
    property var recordingOptions: ({})
    function startRecording(options) {
        recordingOptions = options;
        recordingDelay.restart();
    }
    Timer {
        id: recordingDelay
        interval: 320
        onTriggered: root.work("record", root.recordingOptions.screen ? "screen" : "area", root.recordingOptions)
    }
    Process {
        id: loader
        command: [root.helper, "launcher-workflows", "load"]
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    const value = JSON.parse(text);
                    if (value.error)
                        root.message = value.error;
                    else
                        root.data = value;
                } catch (error) {
                    root.message = NacreI18n.tr("launcher.workflows.read-error", "Could not read launcher workflows");
                }
            }
        }
    }
    Process {
        id: action
        objectName: "launcherWorkflowAction"
        property string operation: ""
        property string value: ""
        property var payload: ({})
        command: [root.helper, "launcher-workflows", operation, value]
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
                    root.completed(!value.error);
                    if (value.error && ["record", "qr", "activate", "share", "annotate", "compress", "gif"].includes(action.operation))
                        AppLaunch.run(["notify-send", "-a", "Nacre", "Nacre", value.error]);
                } catch (error) {
                    root.message = NacreI18n.tr("launcher.workflows.action-error", "Could not complete the launcher action");
                    root.completed(false);
                }
            }
        }
        onExited: {
            stdinEnabled = true;
            root.refresh();
        }
    }
    Process {
        id: stopWorker
        objectName: "launcherRecordingStop"
        command: [root.helper, "launcher-workflows", "record-stop", ""]
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    const value = JSON.parse(text);
                    root.message = value.error || value.message || "";
                    if (value.error)
                        AppLaunch.run(["notify-send", "-a", "Nacre", "Nacre", value.error]);
                } catch (error) {
                    root.message = NacreI18n.tr("launcher.workflows.action-error", "Could not complete the launcher action");
                }
            }
        }
        onExited: root.refresh()
    }
    Component.onCompleted: refresh()
    Timer {
        interval: 10000
        repeat: true
        running: root.active && !root.data.recording
        onTriggered: root.refresh()
    }
    Timer {
        interval: 1000
        repeat: true
        running: root.active && root.data.recording === true
        onTriggered: {
            root.clock = Date.now() / 1000;
            root.refresh();
        }
    }
}
