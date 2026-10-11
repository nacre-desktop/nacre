pragma Singleton
import Quickshell

Singleton {
    readonly property var clients: NacreHyprland.clients
    readonly property var workspaces: NacreHyprland.workspaces
    readonly property var monitors: NacreHyprland.monitors
    readonly property var activeClient: NacreHyprland.activeClient
    readonly property var activeWorkspace: NacreHyprland.activeWorkspace
    readonly property var focusedMonitor: NacreHyprland.focusedMonitor
    readonly property int activeWsId: NacreHyprland.activeWsId
    readonly property point cursorPos: NacreHyprland.cursorPos
    readonly property var toplevels: NacreHyprland.toplevels
    function reload() {
        NacreHyprland.reload();
    }
    function refreshToplevels() {
        NacreHyprland.refreshToplevels();
    }
    function dispatch(request) {
        return NacreHyprland.dispatch(request);
    }
}
