pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io
import "resource-data.js" as Data

Singleton {
    id: root
    readonly property bool visibleDashboard: NacrePanelState.hidden !== true && Object.values(NacrePanelState.screens).some(view => view.dashboard && [0, 3].includes(view.dashboardTab ?? 0))
    property string kernel: ""
    property string loadAverage: ""
    property real cpuPerc: NaN
    property real cpuTemp: NaN
    property bool gpuUsageAvailable: false
    property real gpuPerc: NaN
    property real gpuTemp: NaN
    property real memUsed: 0
    property real memTotal: 0
    readonly property real memPerc: memTotal > 0 ? memUsed / memTotal : NaN
    property real storageUsed: 0
    property real storageTotal: 0
    readonly property real storagePerc: storageTotal > 0 ? storageUsed / storageTotal : NaN
    property var previousCpu: null
    property int fastReads: 0
    property int slowReads: 0
    property string error: ""
    function cpuSample(text) {
        const current = Data.cpu(text);
        cpuPerc = Data.fraction(previousCpu, current);
        if (current)
            previousCpu = current;
    }
    function memorySample(text) {
        const value = Data.memory(text);
        if (value) {
            memTotal = value.total;
            memUsed = value.used;
        }
    }
    function slowSample(text) {
        try {
            const value = JSON.parse(text);
            cpuTemp = Number.isFinite(value.cpuTemp) ? value.cpuTemp : NaN;
            gpuTemp = Number.isFinite(value.gpuTemp) ? value.gpuTemp : NaN;
            gpuUsageAvailable = value.gpuUsageAvailable === true && Number.isFinite(value.gpuPerc) && value.gpuPerc >= 0 && value.gpuPerc <= 1;
            gpuPerc = gpuUsageAvailable ? value.gpuPerc : NaN;
            if (Number.isFinite(value.storageTotal) && Number.isFinite(value.storageUsed) && value.storageTotal > 0 && value.storageUsed >= 0 && value.storageUsed <= value.storageTotal) {
                storageTotal = value.storageTotal;
                storageUsed = value.storageUsed;
            }
            error = "";
        } catch (failure) {
            error = "Some system readings are unavailable.";
        }
    }
    function fastRefresh() {
        fastReads++;
        cpuFile.reload();
        memoryFile.reload();
        loadFile.reload();
    }
    function slowRefresh() {
        if (!slow.running) {
            slowReads++;
            slow.running = true;
        }
    }
    function formatKib(kib) {
        if (!Number.isFinite(kib) || kib < 0)
            return ["—", ""];
        const units = ["KiB", "MiB", "GiB", "TiB"];
        let index = 0, value = kib;
        while (value >= 1024 && index < units.length - 1) {
            value /= 1024;
            index++;
        }
        return [value.toFixed(index ? 1 : 0), units[index]];
    }
    onVisibleDashboardChanged: if (visibleDashboard) {
        previousCpu = null;
        fastRefresh();
        warm.restart();
        slowRefresh();
    } else
        warm.stop()
    Component.onCompleted: {
        fastRefresh();
        slowRefresh();
    }
    FileView {
        id: cpuFile
        path: "/proc/stat"
        printErrors: false
        onLoaded: root.cpuSample(text())
    }
    FileView {
        id: memoryFile
        path: "/proc/meminfo"
        printErrors: false
        onLoaded: root.memorySample(text())
    }
    FileView {
        id: loadFile
        path: "/proc/loadavg"
        printErrors: false
        onLoaded: root.loadAverage = text().trim().split(/\s+/).slice(0, 3).join(" ")
    }
    FileView {
        path: "/proc/sys/kernel/osrelease"
        printErrors: false
        onLoaded: root.kernel = text().trim()
    }
    Timer {
        id: warm
        interval: 250
        onTriggered: if (root.visibleDashboard)
            root.fastRefresh()
    }
    Timer {
        objectName: "resourceFastTimer"
        interval: 1000
        repeat: true
        running: root.visibleDashboard
        onTriggered: root.fastRefresh()
    }
    Timer {
        objectName: "resourceSlowTimer"
        interval: 30000
        repeat: true
        running: root.visibleDashboard
        onTriggered: root.slowRefresh()
    }
    Process {
        id: slow
        objectName: "resourceReader"
        command: ["python3", Quickshell.env("HOME") + "/.local/share/nacre/shell/tools/resource-state.py"]
        stdout: StdioCollector {
            onStreamFinished: root.slowSample(text)
        }
        onExited: (exitCode, exitStatus) => {
            if (exitCode !== 0)
                root.error = "Some system readings are unavailable.";
        }
    }
    IpcHandler {
        target: "resources"
        function state(): string {
            return JSON.stringify({
                visible: root.visibleDashboard,
                fastReads: root.fastReads,
                slowReads: root.slowReads,
                cpu: root.cpuPerc,
                memory: root.memPerc,
                storage: root.storagePerc,
                cpuTemperature: root.cpuTemp,
                gpuAvailable: root.gpuUsageAvailable,
                error: root.error
            });
        }
    }
}
