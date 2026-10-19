pragma Singleton
import QtQuick
import qs.utils
import Quickshell
import Quickshell.Io
import "daemon-state.js" as State

Singleton {
    id: root

    property var favorites: []
    property var hidden: []
    property var recent: []
    property var usage: ({})
    property string error: ""
    property bool ready: false
    property var pending: []
    readonly property var daemonStore: NacreDaemon.enabled ? NacreDaemon.topic("settings.store.launcher") : null
    Connections {
        target: root.daemonStore
        function onDataChanged() {
            const snapshot = root.daemonStore.data;
            if (snapshot.store !== "launcher")
                return;
            if (snapshot.error)
                root.error = snapshot.error.message;
            else {
                root.acceptData(snapshot.data);
                root.ready = true;
            }
        }
    }
    Component.onCompleted: if (NacreDaemon.enabled)
        NacreDaemon.subscribe("settings.store.launcher")
    Component.onDestruction: if (NacreDaemon.enabled)
        NacreDaemon.unsubscribe("settings.store.launcher")

    function update(action, id, enabled) {
        pending.push({
            "action": action,
            "id": id,
            "enabled": enabled
        });
        next();
    }

    function next() {
        if (!worker.running && pending.length) {
            worker.change = pending.shift();
            worker.command = [NacrePaths.helperBinary, "launcher-preferences", worker.change.action];
            worker.running = true;
        }
    }

    function accept(line) {
        try {
            acceptData(JSON.parse(line));
        } catch (e) {
            error = NacreI18n.tr("ui.launcher-preferences.error.could-not-read-launcher-preferences", "Could not read launcher preferences");
        }
    }

    function acceptData(data) {
        try {
            if (data.error) {
                error = data.error;
                return;
            }
            if (!Array.isArray(data.favorites ?? []) || !Array.isArray(data.hidden ?? [])) {
                error = NacreI18n.tr("ui.launcher-preferences.error.invalid-launcher-preferences-original-file-preserved", "Invalid launcher preferences; original file preserved");
                return;
            }
            if (!State.equal(favorites, data.favorites ?? []))
                favorites = State.copy(data.favorites ?? []);

            if (!State.equal(hidden, data.hidden ?? []))
                hidden = State.copy(data.hidden ?? []);

            if (!State.equal(recent, data.recent ?? []))
                recent = State.copy(data.recent ?? []);

            if (!State.equal(usage, data.usage || {}))
                usage = State.copy(data.usage || {});

            error = "";
        } catch (e) {
            error = NacreI18n.tr("ui.launcher-preferences.error.could-not-read-launcher-preferences", "Could not read launcher preferences");
        }
    }

    NacreHelperTask {
        id: reader

        running: !NacreDaemon.enabled
        command: [NacrePaths.helperBinary, "launcher-preferences", "load"]
        onExited: {
            root.ready = true;
            if (!NacreDaemon.enabled)
                watcher.reload();
        }

        onOutput: line => {
            return root.accept(line);
        }
    }

    NacreHelperTask {
        id: worker

        property var change

        stdinEnabled: true
        onStarted: write(JSON.stringify(change) + "\n")
        onExited: root.next()

        onOutput: line => {
            return root.accept(line);
        }
    }

    FileView {
        id: watcher

        preload: root.ready
        path: NacreDaemon.enabled ? "" : NacrePaths.config + "/launcher.json"
        watchChanges: !NacreDaemon.enabled
        onFileChanged: reload()
        onLoaded: if (!NacreDaemon.enabled)
            root.accept(text())
    }
}
