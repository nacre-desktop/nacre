pragma Singleton
import QtQuick
import qs.utils
import Quickshell
import Quickshell.Io

Singleton {
    id: root

    property var favorites: []
    property var hidden: []
    property var recent: []
    property var usage: ({})
    property string error: ""
    property bool ready: false
    property var pending: []

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
            worker.command = ["python3", NacrePaths.tools + "/launcher-preferences.py", worker.change.action];
            worker.running = true;
        }
    }

    function accept(line) {
        try {
            const data = JSON.parse(line);
            if (data.error) {
                error = data.error;
                return;
            }
            if (!Array.isArray(data.favorites ?? []) || !Array.isArray(data.hidden ?? [])) {
                error = NacreI18n.tr("ui.launcher-preferences.error.invalid-launcher-preferences-original-file-preserved", "Invalid launcher preferences; original file preserved");
                return;
            }
            if (JSON.stringify(favorites) !== JSON.stringify(data.favorites ?? []))
                favorites = data.favorites ?? [];

            if (JSON.stringify(hidden) !== JSON.stringify(data.hidden ?? []))
                hidden = data.hidden ?? [];

            if (JSON.stringify(recent) !== JSON.stringify(data.recent ?? []))
                recent = data.recent ?? [];

            if (JSON.stringify(usage) !== JSON.stringify(data.usage || {}))
                usage = data.usage || {};

            error = "";
        } catch (e) {
            error = NacreI18n.tr("ui.launcher-preferences.error.could-not-read-launcher-preferences", "Could not read launcher preferences");
        }
    }

    NacreHelperTask {
        id: reader

        running: true
        command: ["python3", NacrePaths.tools + "/launcher-preferences.py", "load"]
        onExited: {
            root.ready = true;
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
        path: NacrePaths.config + "/launcher.json"
        watchChanges: true
        onFileChanged: reload()
        onLoaded: root.accept(text())
    }
}
