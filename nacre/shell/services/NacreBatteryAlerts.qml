pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Services.UPower
import qs.utils

Singleton {
    id: root
    readonly property var device: UPower.displayDevice
    readonly property var sample: ({
            ready: !!device?.ready && (!device.isLaptopBattery || !device.isPresent || device.state !== UPowerDeviceState.Unknown),
            present: !!device?.isLaptopBattery && device.isPresent,
            discharging: UPower.onBattery && [UPowerDeviceState.Discharging, UPowerDeviceState.PendingDischarge, UPowerDeviceState.Empty].includes(device?.state),
            fraction: device?.percentage
        })
    property bool cacheReady: false
    property bool restoring: false
    property string cacheError: ""
    property string deliveryError: ""
    property int failures: 0
    property int sent: 0
    property var current: null
    readonly property bool busy: sender.running

    function restore(text, missing = false) {
        if (cacheReady)
            return false;
        let level = 0;
        let valid = true;
        if (!missing) {
            try {
                if (text.length > 512)
                    throw new Error();
                const data = JSON.parse(text);
                if (data?.version !== 1 || !Number.isInteger(data.severity) || data.severity < 0 || data.severity > 2)
                    throw new Error();
                level = data.severity;
            } catch (error) {
                cacheError = "Invalid prior battery warning state";
                valid = false;
            }
        }
        restoring = true;
        policy.initialize(level);
        restoring = false;
        cacheReady = true;
        settle.restart();
        return valid;
    }
    function save() {
        if (cacheReady && !restoring)
            ledger.setText(JSON.stringify({
                version: 1,
                severity: policy.severity
            }));
    }
    function evaluate() {
        if (!cacheReady)
            return;
        const request = policy.request(sample);
        if (!request) {
            retry.stop();
            return;
        }
        // Completion reevaluates the latest native snapshot instead of a stale queue.
        if (busy)
            return;
        current = request;
        sender.command = ["notify-send", "--app-name", "Battery", "--urgency", request.level === 2 ? "critical" : "normal", "Battery Low", "Remaining: " + request.percent + "%"];
        sender.running = true;
    }
    function finish(success) {
        if (!current)
            return;
        const request = current;
        current = null;
        watchdog.stop();
        if (success)
            sent++;
        if (request.epoch !== policy.epoch) {
            settle.restart();
            return;
        }
        if (success) {
            policy.delivered(request);
            failures = 0;
            deliveryError = "";
            settle.restart();
        } else {
            failures = Math.min(3, failures + 1);
            deliveryError = "Battery warning delivery failed";
            if (failures < 3)
                retry.restart();
        }
    }
    onSampleChanged: settle.restart()
    NacreBatteryAlertPolicy {
        id: policy
        objectName: "batteryAlertPolicy"
        onSeverityChanged: root.save()
        onRearmed: {
            root.failures = 0;
            root.deliveryError = "";
            retry.stop();
        }
    }
    FileView {
        id: ledger
        objectName: "batteryAlertLedger"
        path: NacrePaths.state + "/battery-alert-state.json"
        printErrors: false
        atomicWrites: true
        watchChanges: false
        onLoaded: root.restore(text())
        onLoadFailed: error => {
            if (error !== FileViewError.FileNotFound)
                root.cacheError = "Cannot read prior battery warning state";
            root.restore("", true);
        }
        onSaveFailed: root.cacheError = "Cannot save battery warning state; restart deduplication unavailable"
        onSaved: root.cacheError = ""
    }
    Timer {
        id: settle
        objectName: "batteryAlertSettle"
        interval: 150
        onTriggered: root.evaluate()
    }
    Timer {
        id: retry
        objectName: "batteryAlertRetry"
        interval: 10000
        onTriggered: root.evaluate()
    }
    Timer {
        id: watchdog
        objectName: "batteryAlertWatchdog"
        interval: 5000
        onTriggered: {
            sender.running = false;
            root.finish(false);
        }
    }
    Process {
        id: sender
        objectName: "batteryAlertSender"
        onRunningChanged: if (running)
            watchdog.restart()
        onExited: (code, status) => root.finish(code === 0 && status === 0)
    }
    IpcHandler {
        target: "batteryAlerts"
        function state(): string {
            return JSON.stringify({
                ready: root.cacheReady && root.sample.ready,
                present: root.sample.present,
                discharging: root.sample.discharging,
                percent: root.sample.ready && Number.isFinite(root.sample.fraction) ? Math.round(root.sample.fraction * 100) : -1,
                severity: policy.severity,
                busy: root.busy,
                sent: root.sent,
                failures: root.failures,
                error: root.cacheError || root.deliveryError
            });
        }
    }
}
