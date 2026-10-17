pragma Singleton
import QtQuick
import qs.utils
import Quickshell
import Quickshell.Io

Singleton {
    id: root
    property bool ready: false
    property bool stopping: false
    property int sequence: 0
    property int retryMs: 250
    property var pending: ({})
    readonly property bool daemonMode: NacreDaemon.enabled
    property string transferDirectory: ""
    property int transferGeneration: 0
    function daemonConnection() {
        if (!daemonMode)
            return;
        const generation = ++transferGeneration;
        ready = false;
        transferDirectory = "";
        fail("Helper connection changed; try again");
        if (!NacreDaemon.connected)
            return;
        NacreDaemon.call("helper.transfer", {}, (value, failure) => {
            if (root.transferGeneration !== generation || !NacreDaemon.connected)
                return;
            if (failure || typeof value?.directory !== "string" || !value.directory.startsWith("/")) {
                console.warn("NacreHelpers: Cannot open private helper transfer lease");
                return;
            }
            root.transferDirectory = value.directory;
            root.ready = true;
        });
    }
    function daemonConfiguration() {
        retry.stop();
        stable.stop();
        ready = false;
        fail("Helper connection changed; try again");
        server.running = !daemonMode;
        if (daemonMode)
            daemonConnection();
        else {
            transferGeneration++;
            transferDirectory = "";
        }
    }
    Component.onCompleted: daemonConfiguration()
    onDaemonModeChanged: daemonConfiguration()
    Connections {
        target: NacreDaemon
        function onConnectedChanged() {
            root.daemonConnection();
        }
    }
    readonly property int count: Object.keys(pending).length
    function submit(task, input) {
        if (daemonMode && !ready) {
            Qt.callLater(() => task.complete(1, 0, JSON.stringify({
                    error: "Helper service is disconnected; try again"
                })));
            return 0;
        }
        if (count >= 64) {
            Qt.callLater(() => task.complete(1, 0, JSON.stringify({
                    error: NacreI18n.tr("ui.helpers.error.helper-queue-is-full", "Helper queue is full")
                })));
            return 0;
        }
        const id = ++sequence;
        const tool = String(task.command[1] || "").split("/").pop().replace(/\.py$/, "");
        const request = {
            id: id,
            tool: tool,
            arguments: task.command.slice(2),
            input: input
        };
        pending = Object.assign({}, pending, {
            [id]: {
                task: task,
                request: request,
                sent: false,
                deadline: Date.now() + 45000
            }
        });
        flush();
        return id;
    }
    function cancel(id) {
        const records = Object.assign({}, pending);
        delete records[id];
        pending = records;
    }
    function flush() {
        if (daemonMode) {
            if (!ready)
                return;
            for (const record of Object.values(pending)) {
                if (!record.sent) {
                    record.sent = true;
                    executeDaemon(record);
                }
            }
            return;
        }
        if (stopping || !ready || !server.running)
            return;
        for (const record of Object.values(pending)) {
            if (!record.sent) {
                record.sent = true;
                server.write(JSON.stringify(record.request) + "\n");
            }
        }
    }
    function packetName(name) {
        return typeof name === "string" && /^packet-[A-Za-z0-9_-]{1,89}$/.test(name);
    }
    function releasePackets(generation, files) {
        if (generation !== transferGeneration || !NacreDaemon.connected || !files.length)
            return;
        NacreDaemon.call("helper.transfer", {
            release: files
        }, (value, failure) => {
            if (failure)
                console.warn("NacreHelpers: Cannot release private helper packets");
        });
    }
    function executeDaemon(record) {
        const generation = transferGeneration;
        const directory = transferDirectory;
        const files = [];
        const finish = (value, failure) => {
            releasePackets(generation, files);
            if (!pending[record.request.id])
                return;
            cancel(record.request.id);
            if (failure || !value || ![0, 1].includes(value.code) || typeof value.output !== "string")
                record.task.complete(1, 0, JSON.stringify({
                    error: failure?.message || "Invalid helper response"
                }));
            else
                record.task.complete(value.code, 0, value.output);
        };
        const execute = params => {
            if (generation !== transferGeneration || !pending[record.request.id]) {
                releasePackets(generation, files);
                return;
            }
            NacreDaemon.call("helper.execute", params, (value, failure) => {
                if (failure || !value?.file) {
                    finish(value, failure);
                } else if (packetName(value.file)) {
                    files.push(value.file);
                    packetIo(directory + "/" + value.file, null, (text, failed) => {
                        let response;
                        try {
                            response = failed ? null : JSON.parse(text);
                        } catch (error) {
                            response = null;
                        }
                        finish(response, null);
                    });
                } else
                    finish(null, null);
            }, 45000);
        };
        const text = JSON.stringify(record.request);
        // Six bytes per UTF-16 unit bounds JSON escaping, including surrogates.
        if (text.length * 6 + 128 <= 384 * 1024) {
            execute({
                request: record.request
            });
            return;
        }
        NacreDaemon.call("helper.transfer", {
            reserve: true
        }, (value, failure) => {
            if (failure || value?.directory !== directory || !packetName(value?.file)) {
                finish(null, failure);
                return;
            }
            files.push(value.file);
            if (generation !== transferGeneration || !pending[record.request.id]) {
                releasePackets(generation, files);
                return;
            }
            packetIo(directory + "/" + value.file, text, (ignored, failed) => {
                if (failed)
                    finish(null, {
                        message: "Cannot write private helper packet"
                    });
                else
                    execute({
                        file: value.file
                    });
            });
        });
    }
    function packetIo(path, text, callback) {
        const file = packetFactory.createObject(root, {
            path: path,
            writing: text !== null,
            callback: callback
        });
        if (!file) {
            callback("", true);
            return;
        }
        if (text !== null)
            file.setText(text);
    }
    Component {
        id: packetFactory
        FileView {
            id: packet
            required property bool writing
            required property var callback
            property bool finished: false
            preload: !writing
            printErrors: false
            atomicWrites: true
            function complete(value, failed) {
                if (finished)
                    return;
                finished = true;
                callback(value, failed);
                Qt.callLater(() => packet.destroy());
            }
            onLoaded: if (!writing)
                complete(text(), false)
            onLoadFailed: if (!writing)
                complete("", true)
            onSaved: if (writing)
                complete("", false)
            onSaveFailed: if (writing)
                complete("", true)
        }
    }
    function fail(message) {
        const records = Object.values(pending);
        pending = ({});
        for (const record of records) {
            try {
                record.task.complete(1, 0, JSON.stringify({
                    error: message
                }));
            } catch (error) {
                console.warn("NacreHelpers: Cannot complete failed helper request:", error);
            }
        }
    }
    function receive(text) {
        if (stopping || !server.running)
            return;
        try {
            if (text.length > 8388608)
                throw new Error();
            const value = JSON.parse(text);
            if (value.ready === true && value.version === 1) {
                ready = true;
                flush();
                stable.restart();
                return;
            }
            if (!Number.isInteger(value.id) || ![0, 1].includes(value.code) || typeof value.output !== "string")
                throw new Error();
            const record = pending[value.id];
            if (!record)
                return;
            cancel(value.id);
            record.task.complete(value.code, 0, value.output);
        } catch (error) {
            ready = false;
            stopping = true;
            fail("Could not read the helper service response");
            server.running = false;
        }
    }
    Process {
        id: server
        objectName: "helperService"
        command: [NacrePaths.helperBinary, "serve"]
        stdinEnabled: true
        running: !root.daemonMode
        stdout: SplitParser {
            onRead: text => root.receive(text)
        }
        onExited: {
            if (root.daemonMode)
                return;
            root.ready = false;
            stable.stop();
            if (!root.stopping)
                root.fail("Helper service stopped; try again");
            root.stopping = false;
            retry.interval = root.retryMs;
            root.retryMs = Math.min(10000, root.retryMs * 2);
            retry.restart();
        }
    }
    Timer {
        id: retry
        onTriggered: if (!root.daemonMode)
            server.running = true
    }
    Timer {
        id: stable
        interval: 10000
        onTriggered: if (root.ready)
            root.retryMs = 250
    }
    Timer {
        interval: 10000
        running: server.running && !root.ready
        onTriggered: {
            root.stopping = true;
            root.fail("Helper service did not start; try again");
            server.running = false;
        }
    }
    Timer {
        interval: 1000
        repeat: true
        running: root.count > 0
        onTriggered: if (Object.values(root.pending).some(record => Date.now() >= record.deadline)) {
            if (root.daemonMode) {
                root.fail("Helper request timed out; try again");
                return;
            }
            root.ready = false;
            root.stopping = true;
            root.fail("Helper request timed out; try again");
            server.running = false;
        }
    }
}
