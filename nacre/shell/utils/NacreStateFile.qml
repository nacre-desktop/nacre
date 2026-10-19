import QtQuick
import Quickshell.Io

// Native writes are guarded against downgrades and preserve pre-migration bytes.
FileView {
    id: root
    // Inject the shared client so pure Qt users need no services singleton.
    property var daemonClient: null
    property string storeId: ""
    property string sourcePath: ""
    readonly property bool daemonMode: daemonClient !== null && daemonClient.enabled && storeId.length > 0
    path: daemonMode ? "" : sourcePath
    signal stateReceived(string value, bool missing)
    signal stateUnavailable(string message)
    // Revisions identify document changes; dirty/exists only change missingness.
    property var projectedRevision: undefined
    property int projectedGeneration: -1
    property bool projectedMissing: false
    property var revision: undefined
    property bool daemonBusy: false
    property var pendingAck: null
    property bool ackFailed: false
    property int connectionGeneration: 0
    property bool componentReady: false
    property string subscribedStoreId: ""
    readonly property string topicName: "settings.store." + storeId
    readonly property var stream: daemonMode ? daemonClient.topic(topicName) : null
    function structurallyEqual(left, right) {
        if (left === right)
            return true;
        if (left === null || right === null || typeof left !== typeof right)
            return false;
        if (Array.isArray(left) || Array.isArray(right)) {
            if (!Array.isArray(left) || !Array.isArray(right) || left.length !== right.length)
                return false;
            for (let i = 0; i < left.length; i++) {
                if (!structurallyEqual(left[i], right[i]))
                    return false;
            }
            return true;
        }
        if (typeof left !== "object")
            return false;
        const leftKeys = Object.keys(left).sort();
        const rightKeys = Object.keys(right).sort();
        if (leftKeys.length !== rightKeys.length)
            return false;
        for (let i = 0; i < leftKeys.length; i++) {
            if (leftKeys[i] !== rightKeys[i] || !structurallyEqual(left[leftKeys[i]], right[rightKeys[i]]))
                return false;
        }
        return true;
    }
    function sameAckContext(ack) {
        return ack && ack.store === storeId && ack.client === daemonClient && ack.generation === connectionGeneration && ack.boot === daemonClient.boot && daemonClient.connected;
    }
    function failPending(message) {
        if (!pendingAck)
            return;
        const ack = pendingAck;
        pendingAck = null;
        daemonBusy = false;
        ackFailed = true;
        stateError = message;
        if (ack.callId && ack.client && ack.client.cancel)
            ack.client.cancel(ack.callId);
        saveFailed(FileViewError.PermissionDenied);
    }
    function checkPending(snapshot) {
        const ack = pendingAck;
        if (!ack || ack.acceptedRevision === undefined)
            return;
        if (!sameAckContext(ack)) {
            failPending("Connection changed before settings were saved");
            return;
        }
        if (!snapshot || snapshot.store !== ack.store || snapshot.rev === undefined)
            return;
        if (snapshot.error) {
            failPending(snapshot.error.message || "Settings could not be saved");
            return;
        }
        if (snapshot.rev < ack.acceptedRevision || snapshot.dirty !== false)
            return;
        if (!snapshot.data || typeof snapshot.data !== "object" || Array.isArray(snapshot.data)) {
            failPending("Settings publication returned an invalid document");
            return;
        }
        for (const field of ack.expected) {
            if (!Object.prototype.hasOwnProperty.call(snapshot.data, field.key) || !structurallyEqual(snapshot.data[field.key], field.value)) {
                failPending("Settings were superseded before durable confirmation");
                return;
            }
        }
        pendingAck = null;
        ackFailed = false;
        saved();
        Qt.callLater(publish);
    }
    function accept(snapshot) {
        if (!snapshot || snapshot.store !== storeId || snapshot.rev === undefined)
            return;
        if (revision !== undefined && snapshot.rev < revision)
            return;
        revision = snapshot.rev;
        stateError = snapshot.error?.message || "";
        if (!stateError) {
            const missing = !snapshot.exists && !snapshot.dirty;
            if (projectedGeneration !== connectionGeneration || projectedRevision !== snapshot.rev || projectedMissing !== missing) {
                projectedGeneration = connectionGeneration;
                projectedRevision = snapshot.rev;
                projectedMissing = missing;
                stateReceived(JSON.stringify(snapshot.data), missing);
            }
        } else {
            stateUnavailable(stateError);
            if (pendingAck && pendingAck.acceptedRevision !== undefined)
                failPending(stateError);
        }
        checkPending(snapshot);
        Qt.callLater(publish);
    }
    function refresh() {
        if (!daemonMode || !daemonClient.connected || daemonBusy)
            return;
        // The shared client subscribes/resubscribes with a full snapshot. Do not
        // race that snapshot with a second settings.get response. Older injected
        // clients without freshness metadata retain the text adapter read path.
        if (stream && typeof stream.fresh === "boolean") {
            if (stream.fresh)
                accept(stream.data);
            return;
        }
        daemonBusy = true;
        const generation = connectionGeneration;
        const store = storeId;
        const client = daemonClient;
        daemonClient.call("settings.get", {
            store: store
        }, (snapshot, failure) => {
            if (generation !== connectionGeneration || store !== storeId || client !== daemonClient)
                return;
            daemonBusy = false;
            if (failure) {
                stateError = failure.message;
                stateUnavailable(stateError);
            } else {
                accept(snapshot);
            }
        });
    }
    readonly property Connections streamChanges: Connections {
        target: root.stream
        function onDataChanged() {
            root.accept(root.stream.data);
        }
    }
    readonly property Connections linkChanges: Connections {
        target: root.daemonClient
        function onConnectedChanged() {
            root.connectionGeneration++;
            root.revision = undefined;
            root.daemonBusy = false;
            if (root.pendingAck)
                root.failPending("Connection changed before settings were saved");
            if (root.daemonClient.connected)
                root.refresh();
        }
        function onNewBoot() {
            root.connectionGeneration++;
            root.revision = undefined;
            root.daemonBusy = false;
            if (root.pendingAck)
                root.failPending("Daemon restarted before settings were saved");
            root.refresh();
        }
    }
    Component.onCompleted: {
        componentReady = true;
        if (daemonMode) {
            daemonClient.subscribe(topicName);
            subscribedStoreId = storeId;
            refresh();
        }
    }
    onStoreIdChanged: {
        if (!componentReady)
            return;
        connectionGeneration++;
        if (pendingAck)
            failPending("Settings store changed before the save completed");
        revision = undefined;
        daemonBusy = false;
        if (daemonClient && subscribedStoreId.length > 0)
            daemonClient.unsubscribe("settings.store." + subscribedStoreId);
        subscribedStoreId = "";
        if (daemonMode) {
            daemonClient.subscribe(topicName);
            subscribedStoreId = storeId;
            refresh();
        }
    }
    Component.onDestruction: {
        if (pendingAck?.callId && pendingAck.client?.cancel)
            pendingAck.client.cancel(pendingAck.callId);
        pendingAck = null;
        if (daemonClient && subscribedStoreId.length > 0)
            daemonClient.unsubscribe("settings.store." + subscribedStoreId);
    }
    property var queuedText: null
    property string stateError: ""
    function saveText(value) {
        queuedText = String(value);
        ackFailed = false;
        publish();
    }
    function publish() {
        if (daemonMode) {
            if (daemonBusy || pendingAck || ackFailed || !daemonClient.connected || queuedText === null || stateError)
                return;
            if (revision === undefined) {
                refresh();
                return;
            }
            let value;
            try {
                value = JSON.parse(queuedText);
            } catch (error) {
                queuedText = null;
                stateError = String(error);
                saveFailed(FileViewError.PermissionDenied);
                return;
            }
            queuedText = null;
            daemonBusy = true;
            const ack = {
                store: storeId,
                client: daemonClient,
                boot: daemonClient.boot,
                generation: connectionGeneration,
                intent: value,
                intentKeys: Object.keys(value).slice(),
                callId: 0,
                acceptedRevision: undefined,
                expected: null
            };
            pendingAck = ack;
            ack.callId = daemonClient.call("settings.publish", {
                store: ack.store,
                value: value,
                rev: revision
            }, (snapshot, failure) => {
                if (pendingAck !== ack)
                    return;
                if (!sameAckContext(ack)) {
                    failPending("Connection changed before settings were saved");
                    return;
                }
                daemonBusy = false;
                if (failure) {
                    failPending(failure.message || "Settings could not be saved");
                    return;
                }
                if (!snapshot || snapshot.store !== ack.store || !Number.isSafeInteger(snapshot.rev) || !snapshot.data || typeof snapshot.data !== "object" || Array.isArray(snapshot.data)) {
                    failPending("Settings publication returned an invalid response");
                    return;
                }
                const keys = ack.intentKeys;
                const expected = [];
                for (const key of keys) {
                    if (!Object.prototype.hasOwnProperty.call(snapshot.data, key)) {
                        failPending("Settings publication omitted an owned field");
                        return;
                    }
                    expected.push({
                        key: key,
                        value: snapshot.data[key]
                    });
                }
                ack.acceptedRevision = snapshot.rev;
                ack.expected = expected;
                revision = snapshot.rev;
                accept(snapshot);
                const latest = stream?.data;
                if (latest && latest.store === ack.store && latest.rev >= ack.acceptedRevision)
                    accept(latest);
            });
            return;
        }
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
