pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

Singleton {
    id: root
    readonly property bool shown: NacrePanelState.providerViews.some(view => (view.detached || NacrePanelState.hidden !== true) && (view.dashboard && view.dashboardTab === 3))
    property var processes: []
    property var disks: []
    property var network: []
    property var previous: null
    property string message: ""
    readonly property string tool: (Quickshell.env("NACRE_DASHBOARD_TOOLS") || Quickshell.env("HOME") + "/.local/share/nacre/shell/tools") + "/dashboard-system.py"
    function refresh() {
        if (!reader.running) {
            reader.command = ["python3", tool];
            reader.running = true;
        }
        networkFile.reload();
    }
    function end(pid, start) {
        if (!reader.running && shown) {
            reader.command = ["python3", tool, "end", String(pid), String(start)];
            reader.running = true;
        }
    }
    function netSample(text) {
        let received = 0, sent = 0;
        for (const line of text.split("\n")) {
            const match = /^\s*([^:]+):\s*(.*)$/.exec(line);
            if (!match || match[1].trim() === "lo")
                continue;
            const fields = match[2].trim().split(/\s+/).map(Number);
            if (fields.length >= 16 && Number.isFinite(fields[0]) && Number.isFinite(fields[8])) {
                received += fields[0];
                sent += fields[8];
            }
        }
        const now = Date.now();
        if (previous && now > previous.time && received >= previous.received && sent >= previous.sent) {
            const scale = 1000 / (now - previous.time);
            network = [...network.slice(-31),
                {
                    down: (received - previous.received) * scale,
                    up: (sent - previous.sent) * scale
                }
            ];
        }
        previous = {
            time: now,
            received: received,
            sent: sent
        };
    }
    onShownChanged: if (shown) {
        previous = null;
        refresh();
    }
    Timer {
        interval: 10000
        repeat: true
        running: root.shown
        onTriggered: root.refresh()
    }
    Timer {
        interval: 2000
        repeat: true
        running: root.shown
        onTriggered: networkFile.reload()
    }
    FileView {
        id: networkFile
        path: "/proc/net/dev"
        printErrors: false
        onLoaded: if (root.shown)
            root.netSample(text())
    }
    Process {
        id: reader
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    const data = JSON.parse(text);
                    root.message = data.error || "";
                    if (!data.error) {
                        root.processes = data.processes || [];
                        root.disks = data.disks || [];
                    }
                } catch (error) {
                    root.message = "System details unavailable";
                }
            }
        }
    }
}
