pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io
import "../modules/settings/settings-catalog.js" as Catalog

Singleton {
    id: root
    property bool created: false
    property bool visible: false
    property var window: null
    property bool focusPending: false
    property int focusAttempts: 0
    property int activation: 0
    property string error: ""
    readonly property var client: (NacreHyprland.clients || []).find(c => c.pid === Quickshell.processId && c.title === "Nacre Settings" && c.wmClass === "org.quickshell") || null
    readonly property bool active: visible && !!window && !window.minimized && (client?.workspace?.id ?? 1) > 0
    readonly property string page: NacrePanelState.settingsPage
    onActiveChanged: NacrePanelState.settingsVisible = active
    function open(section = "") {
        error = "";
        activation++;
        if (section)
            NacrePanelState.settingsPage = Catalog.find(section).id;
        NacrePanelState.clearPopouts();
        const view = NacrePanelState.getForActive();
        if (view)
            NacrePanelState.closeTransient(view);
        created = true;
        visible = true;
        focusPending = true;
        focusAttempts = 0;
        if (window) {
            window.minimized = false;
            if (!minimizeJob.running && (client?.workspace?.id ?? 1) > 0)
                window.activate();
        }
        focusDelay.restart();
    }
    function close() {
        visible = false;
        focusPending = false;
        focusDelay.stop();
    }
    function focusExisting() {
        if (minimizeJob.running)
            return;
        if (!visible || !focusPending)
            return;
        const client = root.client;
        if (client && /^0x[0-9a-f]+$/i.test(client.address)) {
            if ((client.workspace?.id ?? 1) < 0) {
                const workspace = Math.max(1, NacreHyprland.activeWsId);
                NacreHyprland.dispatch('hl.dsp.window.move({window="address:' + client.address + '",workspace="' + workspace + '",follow=false})');
            }
            NacreHyprland.dispatch('hl.dsp.focus({window="address:' + client.address + '"})');
            focusPending = false;
            focusDelay.stop();
        } else if (++focusAttempts >= 12) {
            focusPending = false;
            focusDelay.stop();
        }
    }
    function minimize() {
        const client = root.client;
        if (!client || minimizeJob.running || !/^0x[0-9a-f]+$/i.test(client.address))
            return;
        minimizeJob.generation = activation;
        minimizeJob.command = ["python3", Quickshell.env("HOME") + "/.config/hypr/scripts/window-hide.py", "hide", "--window", client.address, "--quiet"];
        minimizeJob.running = true;
    }
    Process {
        id: minimizeJob
        objectName: "settingsMinimizeProcess"
        property int generation: 0
        onExited: exitCode => {
            if (exitCode === 0 && root.visible && generation !== root.activation) {
                root.focusPending = true;
                focusDelay.restart();
            }
            if (exitCode !== 0)
                root.error = "Could not minimize Settings. You can close it and reopen it from the launcher.";
        }
    }
    Timer {
        id: focusDelay
        interval: 80
        repeat: true
        onTriggered: root.focusExisting()
    }
    IpcHandler {
        target: "settingsApp"
        function open(page: string): void {
            root.open(page);
        }
        function close(): void {
            root.close();
        }
        function state(): string {
            return JSON.stringify({
                visible: root.visible,
                active: root.active,
                created: root.created,
                page: root.page,
                search: root.window?.view.query || "",
                width: root.window?.width || 0,
                height: root.window?.height || 0
            });
        }
    }
    // Older callers use the same app, even before its first window exists.
    IpcHandler {
        target: "settingsView"
        function open(page: string): void {
            root.open(page);
        }
        function state(): string {
            return JSON.stringify({
                visible: root.visible,
                page: root.page,
                search: root.window?.view.query || "",
                width: root.window?.width || 0,
                height: root.window?.height || 0
            });
        }
    }
}
