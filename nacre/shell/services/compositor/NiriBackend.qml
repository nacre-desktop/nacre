pragma Singleton
import QtQuick
import qs.utils
import qs.services
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
            shapedBackgroundBlur: false,
            cursorPosition: false,
            nativePanelDrag: false,
            shortcutRelease: false,
            fullscreenState: false,
            windowHide: false,
            windowTrash: false,
            scratchpad: false,
            windowGroups: true,
            windowForceKill: false,
            pseudoTiling: false,
            layoutSplit: false,
            keymaps: false,
            nonConsumingShortcuts: false,
            bindingIntrospection: false,
            desktopOptions: ["animations", "blur", "shadow", "followMouse", "naturalScroll", "gapsIn", "borderSize", "rounding"],
            monitorConfiguration: true,
            monitorMirroring: false,
            nativeWindowScreenshot: true,
            nightLightController: true
        })
    readonly property string toolsPath: NacrePaths.compositorTools
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
    property bool overviewOpen: false
    property bool overviewHoverSupported: false
    property string overviewHoveredWindowId: ""
    readonly property var overviewHoverWindow: overviewOpen && overviewHoverSupported ? byId[overviewHoveredWindowId] || null : null
    readonly property var overviewSelectedWindow: overviewOpen ? byId[activeWorkspace?.activeWindowId] || null : null
    readonly property var activeClient: byId[activeWindowId] || null
    readonly property int activeWsId: activeWorkspace?.index ?? 1
    readonly property point cursorPos: Qt.point(0, 0)
    property bool connected: false
    property string error: ""
    property int reconnectAttempt: 0
    property var registrations: []
    property var pendingActions: []
    property int actionSequence: 0
    property var activeAction: null
    property var movingWindows: ({})
    signal event(var value)
    signal configurationReloaded
    signal panelGesture(string data)
    function accept(value) {
        if (!value || typeof value !== "object")
            return false;
        if (value.type !== "snapshot") {
            if (value.type === "configuration-reloaded") {
                if (value.failed)
                    error = NacreI18n.tr("ui.niri.error.config-reload-failed", "Niri configuration failed to reload.");
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
        const movements = [];
        const moving = Object.assign({}, movingWindows);
        for (const record of value.windows) {
            if (!record.windowId || next[record.windowId])
                continue;
            const previous = byId[record.windowId]?.record;
            if (connected && previous && previous.moving !== true && record.moving === true) {
                moving[record.windowId] = true;
                movements.push({
                    type: "window-move",
                    phase: "begin",
                    windowId: record.windowId
                });
            } else if (moving[record.windowId] && record.moving !== true && record.geometryKnown === true) {
                delete moving[record.windowId];
                movements.push({
                    type: "window-move",
                    phase: "end",
                    windowId: record.windowId,
                    body: record.geometry,
                    outputId: record.outputId,
                    floating: record.floating
                });
            }
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
        overviewOpen = value.overviewOpen === true;
        overviewHoverSupported = value.overviewHoverSupported === true;
        overviewHoveredWindowId = overviewOpen ? value.overviewHoveredWindowId || "" : "";
        connected = true;
        error = value.configFailed ? NacreI18n.tr("ui.niri.error.config-reload-failed", "Niri configuration failed to reload.") : "";
        reconnectAttempt = 0;
        for (const id of Object.keys(moving)) {
            if (!next[id]) {
                delete moving[id];
                movements.push({
                    type: "window-move",
                    phase: "cancel",
                    windowId: id
                });
            }
        }
        movingWindows = moving;
        for (const window of retired)
            Qt.callLater(() => window.destroy());
        for (const movement of movements)
            event(movement);
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
        command: [NacrePaths.helperBinary, "compositor", "events"]
        stdout: SplitParser {
            onRead: data => {
                try {
                    if (!root.accept(JSON.parse(data)))
                        root.error = NacreI18n.tr("ui.niri.error.invalid-state", "Invalid compositor state.");
                } catch (failure) {
                    root.error = NacreI18n.tr("ui.niri.error.invalid-event", "Invalid compositor event.");
                }
            }
        }
        onExited: (exitCode, exitStatus) => {
            root.connected = false;
            root.movingWindows = {};
            root.event({
                type: "window-move",
                phase: "cancel"
            });
            root.pendingActions = [];
            root.activeAction = null;
            worker.running = false;
            root.error = NacreI18n.tr("ui.niri.error.ipc-disconnected", "Niri IPC disconnected; reconnecting.");
            root.activeWindowId = "";
            root.overviewOpen = false;
            root.overviewHoverSupported = false;
            root.overviewHoveredWindowId = "";
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
            error = connected ? NacreI18n.tr("ui.niri.error.action-queue-full", "Compositor action queue is full.") : NacreI18n.tr("ui.niri.error.ipc-unavailable", "Niri IPC is unavailable.");
            return false;
        }
        pendingActions = [...pendingActions,
            {
                id: ++actionSequence,
                argv: ["action", name, ...args]
            }
        ];
        runNext();
        return true;
    }
    function runNext() {
        if (!connected || activeAction || !pendingActions.length)
            return;
        if (!worker.running) {
            worker.running = true;
            return;
        }
        activeAction = pendingActions[0];
        pendingActions = pendingActions.slice(1);
        worker.write(JSON.stringify(activeAction) + "\n");
    }
    Process {
        id: worker
        objectName: "niriActionWorker"
        stdinEnabled: true
        command: [NacrePaths.helperBinary, "compositor", "serve-actions"]
        environment: ({
                NACRE_COMPOSITOR: "niri"
            })
        stderr: StdioCollector {}
        onStarted: root.runNext()
        stdout: SplitParser {
            onRead: data => {
                try {
                    const response = JSON.parse(data);
                    if (!root.activeAction || response.id !== root.activeAction.id)
                        throw new Error("Unexpected compositor action response");
                    root.activeAction = null;
                    if (!response.ok) {
                        root.pendingActions = [];
                        root.error = NacreI18n.tr("ui.niri.error.action-failed", "Niri action failed: {detail}", {
                            detail: response.error || "Invalid action response"
                        });
                    }
                } catch (failure) {
                    root.activeAction = null;
                    root.pendingActions = [];
                    root.error = NacreI18n.tr("ui.niri.error.action-failed", "Niri action failed: {detail}", {
                        detail: failure.toString()
                    });
                    worker.running = false;
                }
                Qt.callLater(root.runNext);
            }
        }
        onExited: (exitCode, exitStatus) => {
            if (exitCode !== 0 || root.activeAction) {
                root.error = NacreI18n.tr("ui.niri.error.action-failed", "Niri action failed: {detail}", {
                    detail: stderr.text.trim().split("\n").slice(-1)[0]
                });
                root.pendingActions = [];
            }
            root.activeAction = null;
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
        error = NacreI18n.tr("ui.niri.error.operation-unavailable", "This operation is unavailable on niri.");
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
