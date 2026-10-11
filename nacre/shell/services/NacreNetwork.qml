pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

Singleton {
    id: root
    property var snapshot: ({
            wifiEnabled: false,
            wifiInterface: "",
            networks: []
        })
    property string error: ""
    property string monitorError: ""
    property bool pending: false
    property bool received: false
    property int snapshotReads: 0
    property int retrySeconds: 2
    readonly property bool busy: reader.running
    readonly property bool wifiEnabled: snapshot.wifiEnabled
    readonly property string wifiInterface: snapshot.wifiInterface
    readonly property var networks: snapshot.networks
    readonly property var active: networks.find(point => point.active) || null
    readonly property var visibleNetworks: {
        const grouped = new Map();
        for (const point of networks) {
            const key = point.ssid || point.bssid;
            const before = grouped.get(key);
            if (!before || (!before.active && (point.active || point.strength > before.strength)))
                grouped.set(key, point);
        }
        return [...grouped.values()].filter(point => point.ssid || point.active).sort((left, right) => Number(right.active) - Number(left.active) || right.strength - left.strength || left.ssid.localeCompare(right.ssid) || left.bssid.localeCompare(right.bssid));
    }
    function publish(text) {
        try {
            if (text.length > 2097152)
                throw new Error();
            const result = JSON.parse(text);
            if (result.error) {
                error = String(result.error).slice(0, 240);
                return false;
            }
            if (typeof result.wifiEnabled !== "boolean" || typeof result.wifiInterface !== "string" || (result.wifiInterface && !/^[A-Za-z0-9_][A-Za-z0-9_.-]{0,63}$/.test(result.wifiInterface)) || !Array.isArray(result.networks) || result.networks.length > 4096)
                throw new Error();
            for (const point of result.networks) {
                if (!point || typeof point.ssid !== "string" || point.ssid.length > 32 || typeof point.bssid !== "string" || !/^([0-9a-fA-F]{2}:){5}[0-9a-fA-F]{2}$/.test(point.bssid) || typeof point.active !== "boolean" || !Number.isInteger(point.strength) || point.strength < 0 || point.strength > 100 || !Number.isInteger(point.frequency) || point.frequency <= 0 || point.frequency > 100000)
                    throw new Error();
            }
            snapshot = result;
            received = true;
            error = "";
            return true;
        } catch (failure) {
            error = "Could not read Wi-Fi state. Try refreshing or open the connection editor.";
            return false;
        }
    }
    function refresh() {
        if (reader.running) {
            pending = true;
            return;
        }
        debounce.stop();
        received = false;
        error = "";
        snapshotReads++;
        reader.running = true;
    }
    function scheduleRefresh() {
        debounce.restart();
    }
    Component.onCompleted: refresh()
    Timer {
        id: debounce
        interval: 200
        onTriggered: root.refresh()
    }
    Process {
        id: reader
        objectName: "networkReader"
        command: ["python3", Quickshell.env("HOME") + "/.local/share/nacre/shell/tools/network-state.py"]
        stdout: StdioCollector {
            onStreamFinished: root.publish(text)
        }
        onExited: (exitCode, exitStatus) => {
            if ((!root.received || exitCode !== 0) && !root.error)
                root.error = "Wi-Fi refresh failed. The last available state is still shown.";
            if (root.pending) {
                root.pending = false;
                root.scheduleRefresh();
            }
        }
    }
    Process {
        id: monitor
        objectName: "networkMonitor"
        command: ["nmcli", "--colors", "no", "monitor"]
        environment: ({
                LC_ALL: "C"
            })
        running: true
        stdout: SplitParser {
            onRead: {
                root.monitorError = "";
                root.scheduleRefresh();
            }
        }
        onStarted: stable.start()
        onExited: {
            stable.stop();
            root.monitorError = "Network monitoring paused; retrying shortly.";
            retry.interval = root.retrySeconds * 1000;
            root.retrySeconds = Math.min(60, root.retrySeconds * 2);
            retry.start();
        }
    }
    Timer {
        id: stable
        interval: 30000
        onTriggered: if (monitor.running) {
            root.retrySeconds = 2;
            root.monitorError = "";
        }
    }
    Timer {
        id: retry
        onTriggered: monitor.running = true
    }
    IpcHandler {
        target: "networkStatus"
        function state(): string {
            return JSON.stringify({
                enabled: root.wifiEnabled,
                interface: root.wifiInterface,
                count: root.visibleNetworks.length,
                connectedSsid: root.active?.ssid || "",
                connectedRow: root.visibleNetworks.some(point => point.active && point.ssid === root.active?.ssid),
                busy: root.busy,
                error: root.error,
                monitorRunning: monitor.running,
                monitorError: root.monitorError,
                snapshotReads: root.snapshotReads
            });
        }
        function refresh(): void {
            root.refresh();
        }
    }
}
