import QtQuick
import Quickshell.Io

QtObject {
    id: root
    property var command: []
    readonly property string tool: command[1] || ""
    property var transport: null
    property bool running: false
    property bool stdinEnabled: false
    property int starts: 0
    property bool issued: false
    property bool channelReady: false
    property bool stopping: false
    readonly property bool ready: transport ? transport.ready : channelReady
    property var activeTask: null
    property var waiting: []
    signal started
    signal exited(int code, int status)
    readonly property QtObject stdout: QtObject {
        signal read(string data)
    }
    onRunningChanged: if (running) {
        starts++;
        issued = false;
        (transport || root).enqueue(root);
    }
    function enqueue(task) {
        waiting = [...waiting, task];
        next();
    }
    function next() {
        if (stopping || activeTask || !waiting.length)
            return;
        activeTask = waiting[0];
        waiting = waiting.slice(1);
        if (ready)
            Qt.callLater(() => activeTask?.begin());
        else
            server.running = true;
    }
    function begin() {
        if (!running || issued)
            return;
        started();
        if (!stdinEnabled)
            send(command[3] || "");
    }
    function write(text) {
        if (running && !issued)
            send(JSON.parse(text));
    }
    function send(value) {
        const channel = transport || root;
        if (channel.activeTask !== root || !channel.ready)
            return;
        issued = true;
        channel.server.write(JSON.stringify({
            action: command[2],
            value: value
        }) + "\n");
    }
    function complete(code, status, result = null, advance = true) {
        const task = activeTask;
        if (!task)
            return;
        if (result)
            task.stdout.read(JSON.stringify(result));
        activeTask = null;
        task.running = false;
        task.exited(code, status);
        if (advance)
            Qt.callLater(next);
    }
    function receive(text) {
        if (!server.running)
            return;
        try {
            const data = JSON.parse(text);
            if (!data || typeof data !== "object" || Array.isArray(data))
                throw new Error("Invalid publisher packet");
            if (data.ready) {
                channelReady = true;
                activeTask?.begin();
            } else if (data.presentation && activeTask) {
                activeTask.stdout.read(text);
            } else if (data.done && activeTask) {
                if (!data.result || typeof data.result !== "object" || Array.isArray(data.result) || ![0, 1].includes(data.code))
                    throw new Error("Invalid publisher response");
                complete(data.code, 0, data.result);
            } else
                throw new Error("Unexpected publisher packet");
        } catch (error) {
            channelReady = false;
            stopping = true;
            complete(1, 0, {
                error: NacreI18n.tr("ui.palette-task.error.could-not-read-the-palette-publisher-response", "Could not read the palette publisher response")
            }, false);
            server.running = false;
        }
    }
    readonly property Process server: Process {
        objectName: "paletteServer"
        command: ["python3", root.tool, "serve"]
        stdinEnabled: true
        running: root.transport === null
        stdout: SplitParser {
            onRead: text => root.receive(text)
        }
        onExited: (code, status) => {
            root.channelReady = false;
            root.stopping = false;
            root.complete(code || 1, status, null, false);
            Qt.callLater(root.next);
        }
    }
}
