pragma Singleton
import QtQuick
import QtQml.Models
import Quickshell
import Quickshell.Io
import qs.utils

Singleton {
    id: root
    property var documents: ({})
    property var errors: ({})
    property var present: ({})
    property var queue: []
    readonly property var registry: JSON.parse(registryFile.text()).files
    readonly property var ids: registry.filter(item => !!item.settings).map(item => item.id)
    function path(id) {
        const record = registry.find(item => item.id === id);
        return record && record.paths[0].startsWith("nacre/") ? (record.settings.root === "state" ? NacrePaths.state : NacrePaths.config) + "/" + record.paths[0].slice(6) : "";
    }
    function request(id, key = null, value = null) {
        if (!ids.includes(id))
            return false;
        const pending = queue.slice();
        if (key === null && pending.some(item => item.id === id && item.key === null))
            return true;
        pending.push({
            id: id,
            key: key,
            value: value
        });
        queue = pending;
        next();
        return true;
    }
    function next() {
        if (!worker.running && queue.length) {
            worker.change = queue[0];
            queue = queue.slice(1);
            worker.command = [NacrePaths.helperBinary, "state-file", worker.change.key === null ? "load-id" : "set-id", worker.change.id];
            worker.running = true;
        }
    }
    FileView {
        id: registryFile
        path: NacrePaths.tools + "/nacre-state-files.json"
        blockLoading: true
    }
    Instantiator {
        model: root.ids
        delegate: FileView {
            required property string modelData
            path: root.path(modelData)
            preload: true
            watchChanges: true
            printErrors: false
            onLoaded: root.request(modelData)
            onFileChanged: {
                reload();
                root.request(modelData);
            }
            onLoadFailed: root.request(modelData)
        }
    }
    Process {
        id: worker
        property var change: null
        stdinEnabled: true
        onStarted: if (change.key !== null) {
            write(JSON.stringify({
                key: change.key,
                value: change.value
            }) + "\n");
            stdinEnabled = false;
        } else
            stdinEnabled = false
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    const result = JSON.parse(text);
                    if (!result.ok || !result.data)
                        throw new Error(result.error || "Invalid state response");
                    root.present = Object.assign({}, root.present, {
                        [worker.change.id]: result.exists === true
                    });
                    root.documents = Object.assign({}, root.documents, {
                        [worker.change.id]: result.data
                    });
                    root.errors = Object.assign({}, root.errors, {
                        [worker.change.id]: ""
                    });
                } catch (error) {
                    root.errors = Object.assign({}, root.errors, {
                        [worker.change.id]: String(error)
                    });
                }
            }
        }
        onExited: {
            stdinEnabled = true;
            Qt.callLater(root.next);
        }
    }
}
