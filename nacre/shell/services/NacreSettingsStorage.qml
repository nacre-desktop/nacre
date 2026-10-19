pragma Singleton
import QtQuick
import QtQml.Models
import Quickshell
import Quickshell.Io
import qs.utils
import "daemon-state.js" as State

Singleton {
    id: root
    property var documents: ({})
    property var errors: ({})
    property var present: ({})
    property var queue: []
    readonly property bool daemonMode: NacreDaemon.enabled
    property bool daemonBusy: false
    property var revisions: ({})
    readonly property var watchedIds: !daemonMode ? [] : NacrePanelState.settingsVisible ? ids : ["power"]
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
        if (daemonMode) {
            if (daemonBusy || !NacreDaemon.connected || !queue.length)
                return;
            const change = queue[0];
            // A new daemon boot has a new revision namespace. Reads may be
            // repeated; dispatched mutations are never put back in this queue.
            const needRevision = change.key !== null && revisions[change.id] === undefined;
            if (!needRevision)
                queue = queue.slice(1);
            daemonBusy = true;
            const method = change.key === null || needRevision ? "settings.get" : "settings.set";
            const params = method === "settings.get" ? {
                store: change.id
            } : {
                store: change.id,
                key: change.key,
                value: change.value,
                rev: revisions[change.id]
            };
            NacreDaemon.call(method, params, (result, failure) => {
                if (failure) {
                    errors = Object.assign({}, errors, {
                        [change.id]: failure.message
                    });
                    if (needRevision)
                        queue = queue.slice(1);
                } else {
                    acceptDaemon(change.id, result);
                }
                daemonBusy = false;
                Qt.callLater(next);
            });
            return;
        }
        if (!worker.running && queue.length) {
            worker.change = queue[0];
            queue = queue.slice(1);
            worker.command = [NacrePaths.helperBinary, "state-file", worker.change.key === null ? "load-id" : "set-id", worker.change.id];
            worker.running = true;
        }
    }
    function acceptDaemon(id, snapshot) {
        if (!snapshot || snapshot.store !== id || !snapshot.data)
            return;
        if (revisions[id] !== undefined && snapshot.rev < revisions[id])
            return;
        if (revisions[id] !== snapshot.rev)
            revisions = Object.assign({}, revisions, {
                [id]: snapshot.rev
            });
        if (!State.equal(documents[id], snapshot.data))
            documents = Object.assign({}, documents, {
                [id]: State.copy(snapshot.data)
            });
        if (present[id] !== (snapshot.exists === true))
            present = Object.assign({}, present, {
                [id]: snapshot.exists === true
            });
        const message = snapshot.error?.message || "";
        if (errors[id] !== message)
            errors = Object.assign({}, errors, {
                [id]: message
            });
    }
    function refreshDaemon() {
        if (!daemonMode || !NacreDaemon.connected)
            return;
        // Subscriptions deliver full snapshots at creation and after reconnect.
        // Only explicit reads and writes need RPC callbacks in this queue.
        next();
    }
    onWatchedIdsChanged: refreshDaemon()
    Connections {
        target: NacreDaemon
        function onConnectedChanged() {
            if (!NacreDaemon.connected)
                root.revisions = ({});
            root.refreshDaemon();
        }
        function onNewBoot() {
            root.revisions = ({});
            root.refreshDaemon();
        }
    }
    Component.onCompleted: refreshDaemon()
    Instantiator {
        model: root.watchedIds
        delegate: QtObject {
            required property string modelData
            readonly property string topicName: "settings.store." + modelData
            readonly property var stream: NacreDaemon.topic(topicName)
            readonly property Connections changes: Connections {
                target: stream
                function onDataChanged() {
                    root.acceptDaemon(modelData, stream.data);
                }
            }
            Component.onCompleted: {
                NacreDaemon.subscribe(topicName);
                if (NacreDaemon.connected && stream.fresh)
                    root.acceptDaemon(modelData, stream.data);
            }
            Component.onDestruction: NacreDaemon.unsubscribe(topicName)
        }
    }
    FileView {
        id: registryFile
        path: NacrePaths.tools + "/nacre-state-files.json"
        blockLoading: true
    }
    Instantiator {
        model: root.daemonMode ? [] : root.ids
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
