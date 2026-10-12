pragma Singleton
import QtQuick
import Quickshell
import "compositor" as Backend

Singleton {
    id: root
    readonly property var backend: Backend.HyprlandBackend
    readonly property string backendName: backend.backendName
    readonly property var capabilities: backend.capabilities
    readonly property var windows: backend.clients
    readonly property var clients: windows
    readonly property var workspaces: backend.workspaces
    readonly property var monitors: backend.monitors
    readonly property var activeWindow: backend.activeClient
    readonly property var activeClient: activeWindow
    readonly property var activeWorkspace: backend.activeWorkspace
    readonly property var focusedMonitor: backend.focusedMonitor
    readonly property var activeWorkspaceId: activeWorkspace?.id ?? null
    readonly property int activeWsId: activeWorkspace?.id ?? 1
    readonly property point cursorPos: backend.cursorPos
    readonly property string error: backend.error
    signal event(var value)
    signal configurationReloaded
    signal panelGesture(string data)
    function refreshWindows() {
        backend.refreshToplevels();
    }
    function refreshToplevels() {
        refreshWindows();
    }
    function reload() {
        backend.reload();
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
