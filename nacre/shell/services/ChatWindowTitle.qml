pragma Singleton
import Quickshell
import Quickshell.Io
import QtQuick
import qs.utils

Singleton {
    id: root
    readonly property int pid: NacreCompositor.activeWindow?.wmClass === "nacre-ai-task" ? (NacreCompositor.activeWindow?.pid ?? 0) : 0
    property var titles: ({})
    property var threadIds: ({})
    property var pending: []
    property string scannedPids: ""
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
    function scan(force = true) {
        const pids = NacreCompositor.windows.filter(c => c.wmClass === "nacre-ai-task").map(c => c.pid);
        if (pid && !pids.includes(pid))
            pids.unshift(pid);
        const key = pids.slice().sort((a, b) => a - b).join(",");
        if (!force && key === scannedPids)
            return;
        scannedPids = key;
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
        target: NacreCompositor
        function onWindowsChanged() {
            root.scan(false);
        }
    }
    Timer {
        interval: 15000
        running: NacreCompositor.windows.some(c => c.wmClass === "nacre-ai-task")
        repeat: true
        onTriggered: root.scan()
    }
    Component.onCompleted: scan()
    Process {
        id: reader
        property int requestPid
        command: ["python3", NacrePaths.tools + "/window-chat-title.py", requestPid.toString()]
        stdout: SplitParser {
            splitMarker: ""
            onRead: data => {
                try {
                    const record = JSON.parse(data), updated = Object.assign({}, root.titles), ids = Object.assign({}, root.threadIds);
                    updated[reader.requestPid] = record.title;
                    ids[reader.requestPid] = record.threadId;
                    root.titles = updated;
                    root.threadIds = ids;
                } catch (e) {
                    console.warn("ChatWindowTitle: Cannot decode window conversation title:", e);
                }
            }
        }
        onExited: root.next()
    }
}
