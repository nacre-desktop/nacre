pragma Singleton
import Quickshell

Singleton {
    readonly property var clients: NacreCompositor.windows
    readonly property var workspaces: NacreCompositor.workspaces
    readonly property var monitors: NacreCompositor.monitors
    readonly property var activeClient: NacreCompositor.activeWindow
    readonly property var activeWorkspace: NacreCompositor.activeWorkspace
    readonly property var focusedMonitor: NacreCompositor.focusedMonitor
    readonly property int activeWsId: NacreCompositor.activeWsId
    readonly property point cursorPos: NacreCompositor.cursorPos
    function reload() {
        NacreCompositor.refreshState();
    }
    function refreshToplevels() {
        NacreCompositor.refreshWindows();
    }
    function focusWindow(id) {
        return NacreCompositor.focusWindow(id);
    }
}
