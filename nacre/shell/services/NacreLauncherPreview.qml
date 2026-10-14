import QtQuick
import Quickshell
import Quickshell.Io

Scope {
    id: root
    required property string helper
    property var preview: ({})
    property string previewWanted: ""
    property string previewPath: ""
    function inspect(path) {
        previewWanted = path;
        if (previewPath !== path)
            preview = ({});
        previewDelay.restart();
    }
    Process {
        id: previewWorker
        property string path: ""
        command: [root.helper, "launcher-workflows", "preview", path]
        stdout: StdioCollector {
            onStreamFinished: {
                if (previewWorker.path !== root.previewWanted)
                    return;
                try {
                    root.preview = JSON.parse(text);
                    root.previewPath = previewWorker.path;
                } catch (error) {
                    root.preview = ({});
                }
            }
        }
        onExited: if (path !== root.previewWanted)
            previewDelay.restart()
    }
    Timer {
        id: previewDelay
        interval: 120
        onTriggered: if (!previewWorker.running && root.previewWanted) {
            previewWorker.path = root.previewWanted;
            previewWorker.running = true;
        }
    }
}
