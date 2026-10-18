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
    readonly property var daemonStore: NacreDaemon.enabled ? NacreDaemon.topic("settings.store.profile") : null
    function accept(value) {
        if (value.version === 1 && Number.isInteger(value.workspaceCount) && value.workspaceCount > 0 && value.workspaceCount <= 10 && Array.isArray(value.components))
            data = value;
    }
    Connections {
        target: root.daemonStore
        function onDataChanged() {
            const snapshot = root.daemonStore.data;
            if (snapshot.store !== "profile")
                return;
            root.ready = true;
            if (snapshot.error)
                console.warn("NacreProfile:", snapshot.error.message);
            else
                root.accept(snapshot.data);
        }
    }
    Component.onCompleted: if (NacreDaemon.enabled)
        NacreDaemon.subscribe("settings.store.profile")
    Component.onDestruction: if (NacreDaemon.enabled)
        NacreDaemon.unsubscribe("settings.store.profile")
    FileView {
        id: file
        preload: true
        path: NacreDaemon.enabled ? "" : NacrePaths.config + "/profile.json"
        watchChanges: !NacreDaemon.enabled
        printErrors: false
        onFileChanged: reload()
        onLoadFailed: if (!NacreDaemon.enabled)
            root.ready = true
        onLoaded: {
            if (NacreDaemon.enabled)
                return;
            root.ready = true;
            try {
                const value = JSON.parse(text());
                root.accept(value);
            } catch (_) {
                console.warn("NacreProfile: Cannot decode desktop profile:", _);
            }
        }
    }
}
