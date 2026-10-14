pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io
import "../modules/settings/registry/registry.js" as Registry

Singleton {
    id: root
    // Canonical native metadata is used by compositor rules; visible captions localize separately.
    readonly property string nativeTitle: "Nacre Settings"
    readonly property string windowTitle: NacreI18n.tr("applications.settings.title", "Nacre Settings")
    property bool created: false
    property bool visible: false
    property var window: null
    property bool focusPending: false
    property int focusAttempts: 0
    property int activation: 0
    property string error: ""
    readonly property var client: (NacreCompositor.windows || []).find(c => c.pid === Quickshell.processId && c.title === root.nativeTitle && c.wmClass === "org.quickshell") || null
    readonly property bool active: visible && !!window && !window.minimized && (NacreCompositor.capabilities?.perOutputWorkspaces === true || (client?.workspace?.id ?? 1) > 0)
    readonly property string page: NacrePanelState.settingsPage
    onActiveChanged: NacrePanelState.settingsVisible = active
    function open(section = "") {
        error = "";
        activation++;
        if (section) {
            const route = Registry.resolve(NacreSettingsRegistry.schema, section, NacreSettingsCapabilities.snapshot);
            if (!route) {
                error = NacreI18n.tr("settings.registry.ui.invalid-route", "This Settings page is unavailable.");
                return;
            }
            NacrePanelState.settingsPage = route.page + (route.setting ? "#" + route.setting : "");
        }
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
            if (!minimizeJob.running && (NacreCompositor.capabilities?.perOutputWorkspaces === true || (client?.workspace?.id ?? 1) > 0))
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
        if (client && client.windowId) {
            if ((client.workspace?.id ?? 1) < 0) {
                const workspace = Math.max(1, NacreCompositor.activeWsId);
                NacreCompositor.moveWindowToWorkspace(client.windowId, workspace);
            }
            NacreCompositor.focusWindow(client.windowId);
            focusPending = false;
            focusDelay.stop();
        } else if (++focusAttempts >= 12) {
            focusPending = false;
            focusDelay.stop();
        }
    }
    function minimize() {
        if (NacreCompositor.capabilities?.windowHide === false) {
            error = NacreI18n.tr("ui.compositor.error.minimization-unavailable", "Window minimization is unavailable on this compositor.");
            return;
        }
        const client = root.client;
        if (!client || minimizeJob.running || !client.windowId)
            return;
        minimizeJob.generation = activation;
        minimizeJob.command = NacreCompositor.hideWindowCommand(client.windowId);
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
                root.error = NacreI18n.tr("ui.settings-app.error.could-not-minimize-settings-you-can-close-it-and-reopen", "Could not minimize Settings. You can close it and reopen it from the launcher.");
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
                search: root.window?.searchQuery || "",
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
                search: root.window?.searchQuery || "",
                width: root.window?.width || 0,
                height: root.window?.height || 0
            });
        }
    }
}
