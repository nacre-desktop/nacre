pragma Singleton
import QtQuick
import Quickshell
import "compositor" as Backend
import "compositor/session.js" as Session

Singleton {
    id: root
    readonly property string selectedBackend: Session.selectBackend({
        NACRE_COMPOSITOR: Quickshell.env("NACRE_COMPOSITOR"),
        XDG_CURRENT_DESKTOP: Quickshell.env("XDG_CURRENT_DESKTOP"),
        NIRI_SOCKET: Quickshell.env("NIRI_SOCKET"),
        HYPRLAND_INSTANCE_SIGNATURE: Quickshell.env("HYPRLAND_INSTANCE_SIGNATURE")
    })
    readonly property var backend: selectedBackend === "niri" ? Backend.NiriBackend : selectedBackend === "hyprland" ? Backend.HyprlandBackend : null
    readonly property string backendName: backend?.backendName ?? selectedBackend
    readonly property var capabilities: backend?.capabilities ?? ({})
    readonly property var windows: backend?.clients ?? []
    readonly property var clients: windows
    readonly property var workspaces: backend?.workspaces ?? ({
            values: []
        })
    readonly property var monitors: backend?.monitors ?? ({
            values: []
        })
    readonly property var activeWindow: backend?.activeClient ?? null
    readonly property var activeClient: activeWindow
    readonly property var activeWorkspace: backend?.activeWorkspace ?? null
    readonly property var focusedMonitor: backend?.focusedMonitor ?? null
    readonly property var activeWorkspaceId: activeWorkspace?.id ?? null
    readonly property int activeWsId: backend?.capabilities?.perOutputWorkspaces ? backend.activeWorkspace?.index ?? 1 : backend?.activeWorkspace?.id ?? 1
    readonly property point cursorPos: backend?.cursorPos ?? Qt.point(0, 0)
    readonly property string error: backend?.error ?? NacreI18n.tr("ui.compositor.error.unknown-backend", "Unknown compositor backend.")
    signal event(var value)
    signal configurationReloaded
    signal panelGesture(string data)
    function workspaceUrgent(index) {
        return capabilities.perOutputWorkspaces && workspaces.values.some(value => value.outputId === focusedMonitor?.name && value.index === index && value.urgent);
    }
    function windowsForWorkspace(index) {
        if (!capabilities.perOutputWorkspaces)
            return windows.filter(client => client.workspace?.id === index);
        const workspace = workspaces.values.find(value => value.outputId === focusedMonitor?.name && value.index === index);
        return workspace ? windows.filter(client => client.workspaceId === workspace.id) : [];
    }
    function refreshWindows() {
        backend.refreshToplevels();
    }
    function refreshToplevels() {
        refreshWindows();
    }
    function reload() {
        return backend.reload();
    }
    function refreshState() {
        backend.refreshState();
    }
    function createShortcut(owner, params) {
        return backend.createShortcut(owner, params);
    }
    function switchWorkspace(id) {
        return backend.switchWorkspace(id);
    }
    function focusWindow(id) {
        return backend.focusWindow(id);
    }
    function moveWindowToWorkspace(id, workspace, follow = false) {
        return backend.moveWindowToWorkspace(id, workspace, follow);
    }
    function reloadConfiguration() {
        return backend.reloadConfiguration();
    }
    function moveWorkspaceToMonitor(workspace, monitor) {
        return backend.moveWorkspaceToMonitor(workspace, monitor);
    }
    function hideWindowCommand(id) {
        return backend.hideWindowCommand(id);
    }
    function closeWindow(id) {
        return backend.closeWindow(id);
    }
    function floatWindow(id, enabled = null) {
        return backend.floatWindow(id, enabled);
    }
    function pinWindow(id) {
        return backend.pinWindow(id);
    }
    function moveWindow(id, x, y) {
        return backend.moveWindow(id, x, y);
    }
    function resizeWindow(id, width, height) {
        return backend.resizeWindow(id, width, height);
    }
    function placeWindow(id, rect) {
        return backend.placeWindow(id, rect);
    }
    function logout() {
        return backend.logout();
    }
    function publishPanelRegions(pid, regions) {
        return backend.publishPanelRegions(pid, regions);
    }
    Connections {
        target: root.backend
        function onEvent(value) {
            root.event(value);
        }
        function onConfigurationReloaded() {
            root.configurationReloaded();
        }
        function onPanelGesture(data) {
            root.panelGesture(data);
        }
    }
}
