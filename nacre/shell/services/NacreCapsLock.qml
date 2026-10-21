pragma Singleton
import QtQuick
import Qt.labs.folderlistmodel
import Quickshell
import Quickshell.Io

Singleton {
    id: root
    property string device: ""
    property url ledFolder: "file:///sys/class/leds"
    property bool ready: false
    property bool active: false
    readonly property bool available: device !== ""
    signal changed(bool enabled)
    function discover() {
        let next = "";
        for (let index = 0; index < leds.count; index++) {
            const name = String(leds.get(index, "fileName"));
            if (name.endsWith("::capslock")) {
                next = String(leds.get(index, "filePath")) + "/brightness";
                break;
            }
        }
        if (device !== next) {
            ready = false;
            device = next;
        }
    }
    FolderListModel {
        id: leds
        folder: root.ledFolder
        showDotAndDotDot: false
        showFiles: false
        showDirs: true
        onCountChanged: root.discover()
        onStatusChanged: if (status === FolderListModel.Ready)
            root.discover()
    }
    FileView {
        id: state
        path: root.device
        watchChanges: true
        printErrors: false
        onFileChanged: reload()
        onLoaded: {
            const value = text().trim();
            if (!/^[01]$/.test(value))
                return;
            const next = value === "1", notify = root.ready && root.active !== next;
            root.active = next;
            root.ready = true;
            if (notify)
                root.changed(next);
        }
    }
    Timer {
        interval: 250
        repeat: true
        running: root.available && !NacrePanelState.hidden && !WallpaperPlayback.locked
        onTriggered: state.reload()
    }
}
