pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

Singleton {
    id: root
    readonly property string backendName: "niri"
    readonly property var capabilities: ({
            globalWorkspaces: false,
            stableWorkspaceIndices: false,
            perOutputWorkspaces: true,
            scrollingLayout: true,
            floatingPlacement: true,
            pinWindow: false,
            windowCapture: false,
            cursorPosition: false,
            nativePanelDrag: false,
            shortcutRelease: false,
            fullscreenState: false,
            windowHide: false,
            desktopOptions: [],
            monitorConfiguration: false,
            nightLightController: false
        })
    readonly property string toolsPath: Quickshell.env("NACRE_COMPOSITOR_TOOLS") || Quickshell.env("NACRE_PEARL_TOOLS") || Quickshell.env("HOME") + "/.local/share/nacre/shell/tools"
    readonly property var workspaces: ({
            values: workspaceValues
        })
    readonly property var monitors: ({
            values: monitorValues
        })
    property var workspaceValues: []
    property var monitorValues: []
    property var clients: []
    property var byId: ({})
    property var activeWorkspace: null
    property var focusedMonitor: null
    property string activeWindowId: ""
    readonly property var activeClient: byId[activeWindowId] || null
    readonly property int activeWsId: activeWorkspace?.index ?? 1
    readonly property point cursorPos: Qt.point(0, 0)
    property bool connected: false
    property string error: ""
    property int reconnectAttempt: 0
    property var registrations: []
    property var pendingActions: []
    signal event(var value)
    signal configurationReloaded
    signal panelGesture(string data)
    function accept(value) {
        if (!value || typeof value !== "object")
            return false;
        if (value.type !== "snapshot") {
            if (value.type === "configuration-reloaded") {
                if (value.failed)
                    error = qsTr("Niri configuration failed to reload.");
                else
                    configurationReloaded();
            }
            event(value);
            return true;
        }
        if (!Array.isArray(value.windows) || !Array.isArray(value.workspaces) || !Array.isArray(value.monitors))
            return false;
        const next = {};
        const ordered = [];
        for (const record of value.windows) {
            if (!record.windowId || next[record.windowId])
                continue;
            const window = byId[record.windowId] || windowFactory.createObject(root, {
                record: record,
                owner: root
            });
            if (!window)
                continue;
            window.record = record;
            next[record.windowId] = window;
            ordered.push(window);
        }
        const retired = Object.values(byId).filter(window => !next[window.windowId]);
        workspaceValues = value.workspaces;
        monitorValues = value.monitors;
        activeWorkspace = value.activeWorkspace || null;
        focusedMonitor = value.focusedMonitor || null;
        byId = next;
        clients = ordered;
        activeWindowId = value.activeWindow?.windowId || "";
        connected = true;
        error = value.configFailed ? qsTr("Niri configuration failed to reload.") : "";
        reconnectAttempt = 0;
        for (const window of retired)
            Qt.callLater(() => window.destroy());
        return true;
    }
    Component {
        id: windowFactory
        NiriWindow {}
    }
    Component {
        id: shortcutFactory
        NiriShortcut {}
    }
    function createShortcut(owner, params) {
        if (!owner || !params)
            return null;
        const registration = shortcutFactory.createObject(owner, {
            owner: root,
            params: params
        });
        if (registration)
            registrations = [...registrations, registration];
        return registration;
    }
    function unregisterShortcut(registration) {
        registrations = registrations.filter(value => value !== registration);
    }
    function triggerShortcut(name) {
        const targets = registrations.filter(value => value.key() === name);
        for (const registration of targets)
            registration.pulse();
        return targets.length > 0;
    }
    IpcHandler {
        target: "nacreCompositor"
        function shortcut(name: string): bool {
            return root.triggerShortcut(name);
        }
    }
    Process {
        id: stream
        objectName: "niriEventStream"
        running: true
        environment: ({
                NACRE_COMPOSITOR: "niri"
            })
        command: ["python3", root.toolsPath + "/compositor.py", "events"]
        stdout: SplitParser {
            onRead: data => {
                try {
                    if (!root.accept(JSON.parse(data)))
                        root.error = qsTr("Invalid compositor state.");
                } catch (failure) {
                    root.error = qsTr("Invalid compositor event.");
                }
            }
        }
        onExited: (exitCode, exitStatus) => {
            root.connected = false;
            root.error = qsTr("Niri IPC disconnected; reconnecting.");
            root.activeWindowId = "";
            reconnect.restart();
        }
    }
    Timer {
        id: reconnect
        interval: Math.min(5000, 250 * Math.pow(2, Math.min(root.reconnectAttempt, 5)))
        onTriggered: {
            root.reconnectAttempt++;
            stream.running = true;
        }
    }
    function action(name, args = []) {
        if (!connected || pendingActions.length >= 64) {
            error = connected ? qsTr("Compositor action queue is full.") : qsTr("Niri IPC is unavailable.");
            return false;
        }
        pendingActions = [...pendingActions, ["python3", toolsPath + "/compositor.py", "action", name, ...args]];
        runNext();
        return true;
    }
    function runNext() {
        if (worker.running || !pendingActions.length)
            return;
        worker.command = pendingActions[0];
        pendingActions = pendingActions.slice(1);
        worker.running = true;
    }
    Process {
        id: worker
        objectName: "niriActionWorker"
        environment: ({
                NACRE_COMPOSITOR: "niri"
            })
        stderr: StdioCollector {}
        onExited: (exitCode, exitStatus) => {
            if (exitCode !== 0) {
                root.error = qsTr("Niri action failed: %1").arg(stderr.text.trim().split("\n").slice(-1)[0]);
                root.pendingActions = [];
            }
            Qt.callLater(root.runNext);
        }
    }
    function workspaceReference(value) {
        if (value && typeof value === "object")
            return JSON.stringify(value);
        return String(value ?? "");
    }
    function switchWorkspace(value) {
        return action("switch_workspace", ["--workspace", workspaceReference(value)]);
    }
    function knownWindow(id) {
        return typeof id === "string" && !!byId[id];
    }
    function windowAction(name, id, args = []) {
        return knownWindow(id) ? action(name, ["--window", id, ...args]) : false;
    }
    function focusWindow(id) {
        return windowAction("focus_window", id);
    }
    function closeWindow(id) {
        return windowAction("close_window", id);
    }
    function floatWindow(id, enabled = null) {
        return windowAction("float_window", id, ["--action", enabled === true ? "enable" : enabled === false ? "disable" : "toggle"]);
    }
    function moveWindowToWorkspace(id, workspace, follow = false) {
        return windowAction("move_window", id, ["--workspace", workspaceReference(workspace), "--follow", follow ? "true" : "false"]);
    }
    function moveWorkspaceToMonitor(workspace, monitor) {
        return typeof monitor === "string" && monitors.values.some(value => value.name === monitor) ? action("move_workspace", ["--workspace", workspaceReference(workspace), "--monitor", monitor]) : false;
    }
    function moveWindow(id, x, y) {
        return [x, y].every(Number.isFinite) ? windowAction("move_window", id, ["--x", String(Math.round(x)), "--y", String(Math.round(y))]) : false;
    }
    function resizeWindow(id, width, height) {
        return [width, height].every(value => Number.isFinite(value) && value > 0) ? windowAction("resize_window", id, ["--width", String(Math.round(width)), "--height", String(Math.round(height))]) : false;
    }
    function placeWindow(id, rect) {
        return rect && [rect.x, rect.y, rect.width, rect.height].every(Number.isFinite) && rect.width > 0 && rect.height > 0 ? windowAction("place_window", id, ["--x", String(Math.round(rect.x)), "--y", String(Math.round(rect.y)), "--width", String(Math.round(rect.width)), "--height", String(Math.round(rect.height))]) : false;
    }
    function refreshState() {
        // The stream already owns the authoritative cache. Reconnect only if down.
        if (!stream.running && !reconnect.running)
            stream.running = true;
    }
    function refreshToplevels() {
        refreshState();
    }
    function reload() {
        return action("reload");
    }
    function reloadConfiguration() {
        return reload();
    }
    function logout() {
        return action("logout");
    }
    function unsupported() {
        error = qsTr("This operation is unavailable on niri.");
        return false;
    }
    function pinWindow(id) {
        return unsupported();
    }
    function hideWindowCommand(id) {
        unsupported();
        return [];
    }
    function publishPanelRegions(pid, regions) {
        return false;
    }
}
