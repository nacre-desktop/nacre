pragma Singleton
import QtQuick
import qs.utils
import Quickshell
import Quickshell.Io

Singleton {
    id: root
    property bool ready: false
    property var data: ({
            workspaceCount: 6,
            workspaceNames: [],
            workspaceIcons: [],
            components: ["configs", "shell"]
        })
    readonly property bool ai: data.components?.includes("ai") ?? false
    readonly property bool brain: data.components?.includes("brain") ?? false
    FileView {
        id: file
        preload: true
        path: NacrePaths.config + "/profile.json"
        watchChanges: true
        printErrors: false
        onFileChanged: reload()
        onLoadFailed: root.ready = true
        onLoaded: {
            root.ready = true;
            try {
                const value = JSON.parse(text());
                if (value.version === 1 && Number.isInteger(value.workspaceCount) && value.workspaceCount > 0 && value.workspaceCount <= 10 && Array.isArray(value.components))
                    root.data = value;
            } catch (_) {
                console.warn("NacreProfile: Cannot decode desktop profile:", _);
            }
        }
    }
}
