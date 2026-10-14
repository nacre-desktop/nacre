import QtQuick
import Quickshell
import Quickshell.Io

Scope {
    id: root
    required property string helper
    property var preview: ({})
    property string previewWanted: ""
    property string previewPath: ""
    property string emojiWanted: ""
    property string emojiQuery: ""
    property var emojiMatches: []
    function searchEmoji(query) {
        emojiWanted = query;
        if (emojiQuery !== query)
            emojiMatches = [];
        emojiDelay.restart();
    }
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
    Process {
        id: emojiWorker
        objectName: "launcherEmojiSearch"
        property string query: ""
        command: [root.helper, "launcher-workflows", "emoji", query]
        stdout: StdioCollector {
            onStreamFinished: {
                if (emojiWorker.query !== root.emojiWanted)
                    return;
                try {
                    root.emojiMatches = JSON.parse(text).entries || [];
                    root.emojiQuery = emojiWorker.query;
                } catch (error) {
                    root.emojiMatches = [];
                }
            }
        }
        onExited: if (query !== root.emojiWanted)
            emojiDelay.restart()
    }
    Timer {
        id: emojiDelay
        interval: 120
        onTriggered: if (!emojiWorker.running && root.emojiWanted) {
            emojiWorker.query = root.emojiWanted;
            emojiWorker.running = true;
        }
    }
}
