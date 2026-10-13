pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

Singleton {
    id: root
    property bool ready: false
    property bool stopping: false
    property int sequence: 0
    property int retryMs: 250
    property var pending: ({})
    readonly property int count: Object.keys(pending).length
    function submit(task, input) {
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
        if (stopping || !ready || !server.running)
            return;
        for (const record of Object.values(pending)) {
            if (!record.sent) {
                record.sent = true;
                server.write(JSON.stringify(record.request) + "\n");
            }
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
            } catch (error) {}
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
        command: [Quickshell.env("HOME") + "/.local/share/nacre/palette-runtime/venv/bin/python", Quickshell.env("HOME") + "/.local/share/nacre/shell/tools/helper-service.py"]
        stdinEnabled: true
        running: true
        stdout: SplitParser {
            onRead: text => root.receive(text)
        }
        onExited: {
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
        onTriggered: server.running = true
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
            root.ready = false;
            root.stopping = true;
            root.fail("Helper request timed out; try again");
            server.running = false;
        }
    }
}
