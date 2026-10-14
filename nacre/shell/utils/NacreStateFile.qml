import QtQuick
import Quickshell.Io

// Native writes are guarded against downgrades and preserve pre-migration bytes.
FileView {
    id: root
    property var queuedText: null
    property string stateError: ""
    function saveText(value) {
        queuedText = String(value);
        publish();
    }
    function publish() {
        if (publisher.running || queuedText === null)
            return;
        publisher.value = queuedText;
        queuedText = null;
        stateError = "";
        const filename = path.startsWith("file://") ? decodeURIComponent(path.slice(7)) : path;
        publisher.command = [NacrePaths.helperBinary, "state-file", "save", filename];
        publisher.running = true;
    }
    readonly property Process publisher: Process {
        property string value: ""
        stdinEnabled: true
        onStarted: write(value + "\n")
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    root.stateError = JSON.parse(text).error || "";
                } catch (error) {
                    root.stateError = "State writer returned an invalid response";
                }
            }
        }
        onExited: (code, status) => {
            if (code === 0 && !root.stateError) {
                root.saved();
                root.reload();
            } else {
                if (!root.stateError)
                    root.stateError = "State could not be saved";
                root.saveFailed(FileViewError.PermissionDenied);
            }
            Qt.callLater(root.publish);
        }
    }
}
