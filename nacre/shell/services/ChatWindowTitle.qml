pragma Singleton
import Quickshell
import Quickshell.Io
import QtQuick

Singleton {
    id: root
    readonly property int pid: NacreHyprland.activeClient?.wmClass === "nacre-ai-task" ? (NacreHyprland.activeClient?.pid ?? 0) : 0
    property var titles: ({})
    property var threadIds: ({})
    property var pending: []
    IpcHandler {
        target: "chatTitle"
        function state(): string {
            return JSON.stringify({
                pid: root.pid,
                title: root.title,
                resolved: root.resolved,
                cached: Object.keys(root.titles).length
            });
        }
    }
    readonly property string title: titles[pid] ?? ""
    readonly property bool resolved: pid === 0 || titles[pid] !== undefined

    // Resolve open chat windows before focus, and retain their names across switches.
    function scan() {
        const pids = NacreHyprland.clients.filter(c => c.wmClass === "nacre-ai-task").map(c => c.pid);
        if (pid && !pids.includes(pid))
            pids.unshift(pid);
        const retained = {};
        for (const p of pids)
            if (titles[p] !== undefined)
                retained[p] = titles[p];
        titles = retained;
        const ids = {};
        for (const p of pids)
            if (threadIds[p])
                ids[p] = threadIds[p];
        threadIds = ids;
        pending = pids;
        next();
    }
    function next() {
        if (reader.running || pending.length === 0)
            return;
        reader.requestPid = pending.shift();
        reader.running = true;
    }
    onPidChanged: {
        if (pid && titles[pid] === undefined) {
            pending = [pid, ...pending.filter(p => p !== pid)];
            next();
        }
    }
    Connections {
        target: NacreHyprland
        function onClientsChanged() {
            root.scan();
        }
    }
    Timer {
        interval: 15000
        running: NacreHyprland.clients.some(c => c.wmClass === "nacre-ai-task")
        repeat: true
        onTriggered: root.scan()
    }
    Component.onCompleted: scan()
    Process {
        id: reader
        property int requestPid
        command: ["python3", Quickshell.env("HOME") + "/.local/share/nacre/shell/tools/window-chat-title.py", requestPid.toString()]
        stdout: SplitParser {
            splitMarker: ""
            onRead: data => {
                try {
                    const record = JSON.parse(data), updated = Object.assign({}, root.titles), ids = Object.assign({}, root.threadIds);
                    updated[reader.requestPid] = record.title;
                    ids[reader.requestPid] = record.threadId;
                    root.titles = updated;
                    root.threadIds = ids;
                } catch (e) {}
            }
        }
        onExited: root.next()
    }
}
