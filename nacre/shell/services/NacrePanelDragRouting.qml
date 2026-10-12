pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Hyprland as Native

Singleton {
    id: root
    // An event-driven geometry lease. No timer or process runs while idle, and no
    // IPC/IO happens inside Hyprland's pointer binding.
    readonly property string regions: {
        const result = [];
        if (NacrePearls.enabled && DesktopSettings.data.dragPanels !== false && !NacrePanelState.hidden) {
            for (const [name, host] of Object.entries(NacrePanelState.panels)) {
                const screen = NacrePearls.screen(name);
                if (!screen || !host)
                    continue;
                for (const panel of [host.dashboard, host.launcher, host.leftDrawer, host.osd]) {
                    if (!panel?.visible || !panel.enabled || panel.width < 1 || panel.height < 1)
                        continue;
                    // Move events already hold the pointer grab. Publish its
                    // stationary footprint, not a new process per motion frame.
                    const drag = NacrePearls.drag;
                    const rect = drag?.host === panel ? drag.body : {
                        x: screen.x + host.x + panel.x,
                        y: screen.y + host.y + panel.y,
                        width: panel.width,
                        height: panel.height
                    };
                    result.push("{x=" + Math.round(rect.x) + ",y=" + Math.round(rect.y) + ",w=" + Math.round(rect.width) + ",h=" + Math.round(rect.height) + "}");
                }
            }
        }
        return "{" + result.join(",") + "}";
    }
    property string pending: ""
    onRegionsChanged: {
        // Throttle rather than debounce: an opening animation must not postpone
        // its pointer route until the final frame (and make the first pull miss).
        if (!delay.running)
            delay.start();
    }
    Component.onCompleted: delay.restart()
    function publish() {
        pending = "nacre_panel_pid=" + Quickshell.processId + ";nacre_panel_regions=" + regions;
        if (!writer.running)
            writeNext();
    }
    function writeNext() {
        if (!pending)
            return;
        writer.value = pending;
        pending = "";
        writer.running = true;
    }
    Connections {
        target: Native.Hyprland
        function onRawEvent(event) {
            if (event.name === "configreloaded")
                delay.restart();
        }
    }
    Timer {
        id: delay
        interval: 16
        onTriggered: root.publish()
    }
    Process {
        id: writer
        property string value: ""
        command: ["hyprctl", "eval", value]
        onExited: root.writeNext()
    }
}
