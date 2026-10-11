pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Hyprland

Singleton {
    id: root
    property var hardware: ({
            backlight: null,
            keyboard: null,
            ddc: []
        })
    property var monitors: []
    property string error: ""
    readonly property string backlightDevice: hardware.backlight?.device || ""
    readonly property var ddcMonitors: hardware.ddc
    readonly property bool controlsVisible: (NacrePanelState.settingsVisible && ["desktop", "displays"].includes(NacrePanelState.settingsPage)) || Object.values(NacrePanelState.screens).some(view => view.dashboard || view.osd) || Object.values(NacrePanelState.panels).some(panel => panel.feedback?.shown)
    function screenPresent(screen) {
        return Quickshell.screens.some(candidate => candidate === screen || candidate.name === screen?.name);
    }
    function forScreen(screen) {
        if (!screenPresent(screen))
            return null;
        const ddc = hardware.ddc.find(device => device.screen === screen.name);
        if (ddc)
            return ddc;
        const internal = Quickshell.screens.filter(candidate => /^(eDP|LVDS|DSI)/i.test(candidate.name));
        const backlight = hardware.backlight;
        if (backlight?.screen)
            return backlight.screen === screen.name ? backlight : null;
        return internal.length === 1 && internal[0].name === screen.name ? backlight : null;
    }
    function reconcile() {
        const current = [];
        for (const screen of Quickshell.screens) {
            const old = monitors.find(monitor => monitor.modelData === screen);
            current.push(old || factory.createObject(root, {
                modelData: screen
            }));
        }
        const retired = monitors.filter(monitor => !current.includes(monitor));
        monitors = current;
        for (const monitor of retired) {
            monitor.present = false;
            Qt.callLater(() => monitor.destroy());
        }
    }
    function discover() {
        if (discovery.running) {
            discoveryPending = true;
            return;
        }
        discovery.command = ["python3", Quickshell.env("HOME") + "/.local/share/nacre/shell/tools/light-devices.py", "discover", JSON.stringify(Quickshell.screens.map(screen => screen.name))];
        discovery.running = true;
    }
    property bool discoveryPending: false
    function getMonitorForScreen(screen) {
        return monitors.find(monitor => monitor.modelData === screen || monitor.modelData.name === screen?.name) || null;
    }
    function focused() {
        const name = NacreHyprland.focusedMonitor?.name;
        return monitors.find(monitor => monitor.modelData.name === name) || monitors[0] || null;
    }
    function step(direction) {
        const monitor = focused();
        if (!monitor || !["up", "down"].includes(direction))
            return;
        monitor.setBrightness(monitor.brightness + (direction === "up" ? .05 : -.05));
    }
    function increaseBrightness() {
        step("up");
    }
    function decreaseBrightness() {
        step("down");
    }
    Component.onCompleted: {
        reconcile();
        discover();
    }
    Component {
        id: factory
        NacreBacklight {
            owner: root
        }
    }
    Connections {
        target: Quickshell
        function onScreensChanged() {
            root.reconcile();
            root.discover();
        }
    }
    onControlsVisibleChanged: {
        if (controlsVisible)
            for (const monitor of monitors)
                if (!monitor.isDdc)
                    monitor.readNative();
    }
    Timer {
        objectName: "screenLightTimer"
        interval: 3000
        repeat: true
        running: root.controlsVisible
        onTriggered: {
            for (const monitor of root.monitors)
                if (!monitor.isDdc)
                    monitor.readNative();
        }
    }
    Process {
        id: discovery
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    const data = JSON.parse(text);
                    if (data.error) {
                        root.error = data.error;
                        return;
                    }
                    if (!Array.isArray(data.ddc))
                        throw new Error();
                    root.hardware = data;
                    root.error = "";
                } catch (failure) {
                    root.error = "Could not read display light devices.";
                }
            }
        }
        onExited: {
            if (root.discoveryPending) {
                root.discoveryPending = false;
                root.discover();
            }
        }
    }
    GlobalShortcut {
        appid: "nacre_shell"
        name: "brightnessUp"
        onPressed: root.increaseBrightness()
    }
    GlobalShortcut {
        appid: "nacre_shell"
        name: "brightnessDown"
        onPressed: root.decreaseBrightness()
    }
    IpcHandler {
        target: "brightness"
        function step(direction: string): void {
            root.step(direction);
        }
        function set(value: real): string {
            const monitor = root.focused();
            return monitor?.setBrightness(value) ? "ok" : "unavailable";
        }
        function state(): string {
            return JSON.stringify({
                monitors: root.monitors.map(monitor => ({
                            name: monitor.modelData.name,
                            available: monitor.available,
                            brightness: monitor.brightness,
                            isDdc: monitor.isDdc
                        })),
                visible: root.controlsVisible,
                error: root.error
            });
        }
    }
}
