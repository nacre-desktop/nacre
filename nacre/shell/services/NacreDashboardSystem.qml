pragma Singleton
import QtQuick
import qs.utils
import Quickshell
import Quickshell.Io

Singleton {
    id: root
    readonly property bool shown: NacrePanelState.providerViews.some(view => (view.detached || NacrePanelState.hidden !== true) && (view.dashboard && [3, 8].includes(view.dashboardTab)))
    property var processes: []
    property var disks: []
    property var mountSources: ({})
    property var previousProcesses: ({})
    property var cpuByPid: ({})
    property real processSampleTime: 0
    readonly property var filesystems: {
        const seen = new Set();
        return disks.filter(disk => {
            const device = mountSources[disk.path] || disk.path;
            if (seen.has(device))
                return false;
            seen.add(device);
            return true;
        });
    }
    FileView {
        path: "/proc/mounts"
        printErrors: false
        onLoaded: {
            const sources = {};
            for (const line of text().split("\n")) {
                const fields = line.split(/\s+/);
                if (fields.length > 2)
                    sources[fields[1].replace(/\\040/g, " ")] = fields[0].split("[")[0];
            }
            root.mountSources = sources;
        }
    }
    property string routeInterface: ""
    readonly property string interfaceName: NacreNetwork.active && NacreNetwork.wifiInterface ? NacreNetwork.wifiInterface : routeInterface
    onInterfaceNameChanged: {
        previous = null;
        network = [];
    }
    FileView {
        id: routeFile
        path: "/proc/net/route"
        printErrors: false
        onLoaded: {
            const row = text().split("\n").map(line => line.trim().split(/\s+/)).find(fields => fields[1] === "00000000");
            root.routeInterface = row?.[0] || "";
        }
    }
    property var network: []
    property var previous: null
    property string message: ""
    readonly property string tool: "dashboard-system"
    function refresh() {
        if (!reader.running) {
            reader.command = [NacrePaths.helperBinary, tool];
            reader.running = true;
        }
        routeFile.reload();
        networkFile.reload();
    }
    function end(pid, start) {
        if (!reader.running && shown) {
            reader.command = [NacrePaths.helperBinary, tool, "end", String(pid), String(start)];
            reader.running = true;
        }
    }
    function netSample(text) {
        let received = 0, sent = 0;
        for (const line of text.split("\n")) {
            const match = /^\s*([^:]+):\s*(.*)$/.exec(line);
            if (!match || match[1].trim() === "lo" || root.interfaceName && match[1].trim() !== root.interfaceName)
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
        previousProcesses = ({});
        cpuByPid = ({});
        processSampleTime = 0;
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
                        const now = Date.now() / 1000, next = {}, usage = {};
                        for (const row of data.processes || []) {
                            const key = row.pid + ":" + row.start, before = root.previousProcesses[key];
                            if (before !== undefined && now > root.processSampleTime)
                                usage[key] = Math.max(0, (row.cpuSeconds - before) * 100 / (now - root.processSampleTime));
                            next[key] = row.cpuSeconds;
                        }
                        root.cpuByPid = usage;
                        root.previousProcesses = next;
                        root.processSampleTime = now;
                        root.processes = data.processes || [];
                        root.disks = data.disks || [];
                    }
                } catch (error) {
                    root.message = NacreI18n.tr("ui.dashboard-system.message.system-details-unavailable", "System details unavailable");
                }
            }
        }
    }
}
