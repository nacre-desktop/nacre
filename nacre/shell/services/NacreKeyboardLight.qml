pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

Singleton {
    id: root
    readonly property string device: channel.device
    readonly property int maximum: channel.maximum
    readonly property real brightness: channel.brightness
    readonly property bool available: channel.available
    readonly property string error: channel.error
    readonly property bool controlsVisible: NacreBrightness.controlsVisible
    signal adjusted
    signal externalAdjusted
    NacreLightChannel {
        id: channel
        descriptor: NacreBrightness.hardware.keyboard
        present: !!descriptor
        onAdjusted: root.adjusted()
        onExternalAdjusted: root.externalAdjusted()
    }
    function refresh() {
        channel.readNative();
    }
    function readNative() {
        channel.readNative();
    }
    function setBrightness(value) {
        return channel.setBrightness(value);
    }
    function applyDaemonLevel(value, max) {
        if (!Number.isFinite(value) || !Number.isInteger(max) || max !== maximum || max <= 0)
            return false;
        channel.brightness = Math.max(0, Math.min(1, value));
        return true;
    }
    function step(direction) {
        if (!available || !["up", "down"].includes(direction))
            return;
        const raw = Math.round(brightness * maximum);
        setBrightness((direction === "up" ? (raw + 1) % (maximum + 1) : Math.max(0, raw - 1)) / maximum);
    }
    onControlsVisibleChanged: if (controlsVisible)
        readNative()
    Timer {
        objectName: "keyboardLightTimer"
        interval: 3000
        repeat: true
        running: root.controlsVisible
        onTriggered: root.readNative()
    }
    IpcHandler {
        id: keyboardLightIpc
        Component.onCompleted: NacreDaemon.registerIpc("keyboardLight", keyboardLightIpc, {
            "step": ["s"],
            "set": ["r"],
            "state": []
        })
        Component.onDestruction: NacreDaemon.unregisterIpc("keyboardLight", keyboardLightIpc)
        target: "keyboardLight"
        function step(direction: string): void {
            root.step(direction);
        }
        function set(value: real): void {
            root.setBrightness(value);
        }
        function state(): string {
            return JSON.stringify({
                device: root.device,
                maximum: root.maximum,
                brightness: root.brightness,
                available: root.available,
                visible: root.controlsVisible,
                error: root.error
            });
        }
    }
}
