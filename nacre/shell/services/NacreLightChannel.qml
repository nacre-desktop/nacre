import QtQuick
import Quickshell
import Quickshell.Io

QtObject {
    id: root
    property var descriptor: null
    property bool present: true
    property real brightness: 0
    property int pending: -1
    property int epoch: 0
    property int inFlightEpoch: 0
    property int readEpoch: 0
    property string error: ""
    property bool readable: true
    property int nativeMaximum: -1
    readonly property int maximum: nativeMaximum >= 0 ? nativeMaximum : descriptor?.maximum || 0
    readonly property bool isDdc: descriptor?.kind === "ddc"
    readonly property string device: descriptor?.device || ""
    readonly property bool available: readable && present && maximum > 0 && (isDdc || !!device)
    signal adjusted
    onDescriptorChanged: {
        epoch++;
        readEpoch = epoch;
        nativeMaximum = -1;
        readable = true;
        pending = -1;
        delay.stop();
        if (descriptor?.maximum > 0)
            brightness = descriptor.current / descriptor.maximum;
        error = "";
    }
    onPresentChanged: if (!present) {
        epoch++;
        pending = -1;
        delay.stop();
    }
    function parseCurrent(text) {
        const raw = Number(text.trim());
        if (text.trim() && Number.isInteger(raw) && raw >= 0 && raw <= maximum && readEpoch === epoch && pending < 0 && !writer.running) {
            brightness = raw / maximum;
            readable = true;
        }
    }
    function readNative() {
        if (present && maximum > 0 && device && !isDdc) {
            readEpoch = epoch;
            current.reload();
        }
    }
    function refresh() {
        if (!available)
            return;
        if (!isDdc)
            readNative();
        else if (!reader.running) {
            reader.command = ["python3", Quickshell.env("HOME") + "/.local/share/nacre/shell/tools/light-devices.py", "read-ddc", JSON.stringify(descriptor)];
            reader.running = true;
        }
    }
    function setBrightness(value) {
        if (!available || !Number.isFinite(value))
            return false;
        const minimum = descriptor.kind === "backlight" ? 1 : 0;
        pending = Math.max(minimum, Math.min(maximum, Math.round(Math.max(0, Math.min(1, value)) * maximum)));
        epoch++;
        brightness = pending / maximum;
        error = "";
        adjusted();
        delay.restart();
        return true;
    }
    function startWrite() {
        if (!available || pending < 0 || writer.running)
            return;
        const value = pending;
        pending = -1;
        inFlightEpoch = epoch;
        writer.command = ["python3", Quickshell.env("HOME") + "/.local/share/nacre/shell/tools/light-devices.py", "set", JSON.stringify(Object.assign({}, descriptor, {
                maximum: maximum
            })), String(value)];
        writer.running = true;
    }
    function accept(text, expected) {
        try {
            const data = JSON.parse(text);
            if (expected !== epoch)
                return false;
            if (data.error) {
                error = data.error;
                return false;
            }
            if (!Number.isInteger(data.current) || data.current < 0 || data.current > maximum || data.maximum !== maximum)
                return false;
            brightness = data.current / maximum;
            error = "";
            return true;
        } catch (failure) {
            error = "Could not read light control status.";
            return false;
        }
    }
    readonly property Timer delay: Timer {
        interval: 40
        onTriggered: root.startWrite()
    }
    readonly property FileView current: FileView {
        path: root.present && root.device && !root.isDdc ? "/sys/class/" + (root.descriptor.kind === "backlight" ? "backlight" : "leds") + "/" + root.device + "/brightness" : ""
        printErrors: false
        watchChanges: true
        onFileChanged: root.readNative()
        onLoaded: root.parseCurrent(text())
        onLoadFailed: {
            root.readable = false;
            root.error = "Light device is unavailable.";
        }
    }
    readonly property FileView maximumFile: FileView {
        path: root.present && root.device && !root.isDdc ? "/sys/class/" + (root.descriptor.kind === "backlight" ? "backlight" : "leds") + "/" + root.device + "/max_brightness" : ""
        printErrors: false
        onLoaded: {
            const value = Number(text().trim());
            if (text().trim() && Number.isInteger(value) && value >= 0) {
                if (root.nativeMaximum >= 0 && root.nativeMaximum !== value) {
                    root.epoch++;
                    root.pending = -1;
                }
                root.nativeMaximum = value;
            }
        }
    }
    readonly property Process writer: Process {
        command: []
        stdout: StdioCollector {
            onStreamFinished: root.accept(text, root.inFlightEpoch)
        }
        onExited: (exitCode, exitStatus) => {
            if (exitCode !== 0 && root.inFlightEpoch === root.epoch)
                root.error = "Light change failed; the current value will be restored.";
            if (root.pending >= 0)
                delay.restart();
            else
                root.refresh();
        }
    }
    readonly property Process reader: Process {
        id: ddcReader
        property int generation: 0
        onStarted: generation = root.epoch
        stdout: StdioCollector {
            onStreamFinished: root.accept(text, ddcReader.generation)
        }
    }
}
