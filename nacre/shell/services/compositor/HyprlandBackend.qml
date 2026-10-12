pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Hyprland as Native

Singleton {
    id: root
    signal event(var value)
    signal configurationReloaded
    signal panelGesture(string data)
    readonly property string backendName: "hyprland"
    readonly property var capabilities: ({
            globalWorkspaces: true,
            stableWorkspaceIndices: true,
            perOutputWorkspaces: false,
            scrollingLayout: false,
            floatingPlacement: true,
            pinWindow: true,
            windowCapture: true
        })
    property var clients: []
    property var byAddress: ({})
    property var urgentAddresses: ({})
    readonly property var workspaces: Native.Hyprland.workspaces
    readonly property var monitors: Native.Hyprland.monitors
    readonly property var toplevels: Native.Hyprland.toplevels
    readonly property var focusedMonitor: Native.Hyprland.focusedMonitor
    readonly property var activeWorkspace: focusedMonitor?.activeWorkspace || Native.Hyprland.focusedWorkspace || null
    readonly property int activeWsId: activeWorkspace?.id ?? 1
    property bool focusKnown: false
    property string focusAddress: ""
    property int focusRevision: 0
    property bool monitorRefreshPending: false
    readonly property var activeClient: focusKnown ? clients.find(client => client.address.toLowerCase() === focusAddress) || null : clients.find(client => client.nativeWindow?.activated) || (Native.Hyprland.activeToplevel ? clients.find(client => client.nativeWindow?.handle === Native.Hyprland.activeToplevel) : null) || null
    property point cursorPos: Qt.point(0, 0)
    property bool reconcilePending: false
    property int refreshCount: 0
    property string error: ""
    function addressOf(address) {
        const clean = String(address || "").trim().replace(/^0x/i, "").toLowerCase();
        return /^[0-9a-f]+$/.test(clean) ? "0x" + clean : "";
    }
    function observeFocus(address) {
        focusAddress = addressOf(address);
        focusKnown = true;
        clearUrgency(focusAddress);
    }
    function clearUrgency(address) {
        const clean = addressOf(address);
        if (urgentAddresses[clean]) {
            const next = Object.assign({}, urgentAddresses);
            delete next[clean];
            urgentAddresses = next;
        }
    }
    function observeUrgency(address) {
        const clean = addressOf(address);
        const client = byAddress[clean];
        if (client && client !== activeClient && !urgentAddresses[clean])
            urgentAddresses = Object.assign({}, urgentAddresses, {
                [clean]: true
            });
    }
    function reconcile() {
        const next = {};
        const ordered = [];
        for (const window of Native.Hyprland.toplevels.values) {
            const address = addressOf(window.address);
            if (!address || next[address])
                continue;
            const existing = byAddress[address];
            const client = (existing?.nativeWindow === window ? existing : null) || factory.createObject(root, {
                nativeWindow: window,
                address: address
            });
            client.nativeWindow = window;
            next[address] = client;
            ordered.push(client);
        }
        const retired = Object.values(byAddress).filter(client => next[client.address] !== client);
        for (const client of retired)
            clearUrgency(client.address);
        byAddress = next;
        if (ordered.length !== clients.length || ordered.some((client, index) => client !== clients[index]))
            clients = ordered;
        for (const client of retired)
            Qt.callLater(() => client.destroy());
    }
    function scheduleReconcile() {
        if (reconcilePending)
            return;
        reconcilePending = true;
        Qt.callLater(() => {
            reconcilePending = false;
            reconcile();
        });
    }
    function refreshToplevels() {
        refreshCount++;
        Native.Hyprland.refreshToplevels();
    }
    function reload() {
        Native.Hyprland.refreshMonitors();
        Native.Hyprland.refreshWorkspaces();
        refreshToplevels();
        if (!cursor.running)
            cursor.running = true;
    }
    function dispatch(request) {
        if (typeof request !== "string" || !request.trim())
            return false;
        let command = request.trim();
        if (Native.Hyprland.usingLua) {
            const workspace = /^workspace\s+(\d+)$/.exec(command);
            if (workspace && Number(workspace[1]) > 0)
                command = "hl.dsp.focus({workspace=" + Number(workspace[1]) + ",on_current_monitor=true})";
            else if (!command.startsWith("hl.dsp.")) {
                error = "Unsupported workspace/window command for the Lua session.";
                return false;
            }
        }
        Native.Hyprland.dispatch(command);
        error = "";
        return true;
    }
    function focusWindow(address) {
        const clean = addressOf(address);
        if (!clean || !byAddress[clean])
            return false;
        Native.Hyprland.dispatch(Native.Hyprland.usingLua ? "hl.dsp.focus({window=" + JSON.stringify("address:" + clean) + "})" : "focuswindow address:" + clean);
        return true;
    }
    function switchWorkspace(id) {
        return Number.isInteger(Number(id)) && Number(id) > 0 ? dispatch("workspace " + Number(id)) : false;
    }
    function windowSelector(id) {
        const clean = addressOf(id);
        return clean && byAddress[clean] ? "address:" + clean : "";
    }
    function moveWindowToWorkspace(id, workspace, follow = false) {
        const selector = windowSelector(id);
        if (!selector || !Number.isInteger(Number(workspace)) || Number(workspace) < 1)
            return false;
        return dispatch(Native.Hyprland.usingLua ? "hl.dsp.window.move({window=" + JSON.stringify(selector) + ",workspace=" + JSON.stringify(String(workspace)) + ",follow=" + (follow ? "true" : "false") + "})" : (follow ? "movetoworkspace " : "movetoworkspacesilent ") + Number(workspace) + "," + selector);
    }
    function windowAction(id, luaAction, classicAction) {
        const selector = windowSelector(id);
        if (!selector)
            return false;
        return dispatch(Native.Hyprland.usingLua ? "hl.dsp.window." + luaAction + "({window=" + JSON.stringify(selector) + "})" : classicAction + " " + selector);
    }
    function moveWorkspaceToMonitor(workspace, monitor) {
        if (!Number.isInteger(Number(workspace)) || Number(workspace) < 1 || typeof monitor !== "string" || !monitors.values.some(value => value.name === monitor))
            return false;
        return dispatch(Native.Hyprland.usingLua ? "hl.dsp.workspace.move({workspace=" + JSON.stringify(String(workspace)) + ",monitor=" + JSON.stringify(monitor) + "})" : "moveworkspacetomonitor " + Number(workspace) + " " + monitor);
    }
    function reloadConfiguration() {
        reloadConfig.running = true;
        return true;
    }
    Process {
        id: reloadConfig
        objectName: "compositorConfigurationReload"
        command: ["hyprctl", "reload"]
    }
    function hideWindowCommand(id) {
        const selector = windowSelector(id);
        return selector ? ["python3", Quickshell.env("HOME") + "/.config/hypr/scripts/window-hide.py", "hide", "--window", addressOf(id), "--quiet"] : [];
    }
    function closeWindow(id) {
        return windowAction(id, "close", "closewindow");
    }
    function floatWindow(id, enabled = null) {
        const selector = windowSelector(id);
        if (!selector)
            return false;
        const action = enabled === true ? "enable" : enabled === false ? "disable" : "toggle";
        return dispatch(Native.Hyprland.usingLua ? "hl.dsp.window.float({window=" + JSON.stringify(selector) + ",action=" + JSON.stringify(action) + "})" : (action === "enable" ? "setfloating " : action === "disable" ? "settiled " : "togglefloating ") + selector);
    }
    function pinWindow(id) {
        return windowAction(id, "pin", "pin");
    }
    function geometryAction(id, action, x, y) {
        const selector = windowSelector(id);
        if (!selector || !Number.isFinite(x) || !Number.isFinite(y) || (action === "resize" && (x <= 0 || y <= 0)))
            return false;
        const dimensions = "x=" + Math.round(x) + ",y=" + Math.round(y);
        return dispatch(Native.Hyprland.usingLua ? "hl.dsp.window." + action + "({window=" + JSON.stringify(selector) + "," + dimensions + ",relative=false})" : (action === "resize" ? "resizewindowpixel exact " : "movewindowpixel exact ") + Math.round(x) + " " + Math.round(y) + "," + selector);
    }
    function moveWindow(id, x, y) {
        return geometryAction(id, "move", x, y);
    }
    function resizeWindow(id, width, height) {
        return geometryAction(id, "resize", width, height);
    }
    function placeWindow(id, rect) {
        if (!windowSelector(id) || !rect || ![rect.x, rect.y, rect.width, rect.height].every(Number.isFinite) || rect.width <= 0 || rect.height <= 0)
            return false;
        return floatWindow(id, true) && resizeWindow(id, rect.width, rect.height) && moveWindow(id, rect.x, rect.y);
    }
    function logout() {
        Native.Hyprland.dispatch(Native.Hyprland.usingLua ? "hl.dsp.exit()" : "exit");
        return true;
    }
    function publishPanelRegions(pid, regions) {
        if (!Number.isInteger(pid) || pid <= 0 || !Array.isArray(regions) || regions.some(rect => !rect || ![rect.x, rect.y, rect.width, rect.height].every(Number.isFinite)))
            return false;
        panelWriter.pending = "nacre_panel_pid=" + pid + ";nacre_panel_regions={" + regions.map(rect => "{x=" + Math.round(rect.x) + ",y=" + Math.round(rect.y) + ",w=" + Math.round(rect.width) + ",h=" + Math.round(rect.height) + "}").join(",") + "}";
        writePanelRegions();
        return true;
    }
    function writePanelRegions() {
        if (!panelWriter.running && panelWriter.pending) {
            panelWriter.value = panelWriter.pending;
            panelWriter.pending = "";
            panelWriter.running = true;
        }
    }
    Process {
        id: panelWriter
        objectName: "panelGeometryWriter"
        property string pending: ""
        property string value: ""
        command: ["hyprctl", "eval", value]
        onExited: root.writePanelRegions()
    }
    Component.onCompleted: {
        reconcile();
        if (!cursor.running)
            cursor.running = true;
        focusReader.running = true;
    }
    Component {
        id: factory
        HyprlandWindow {
            owner: root
        }
    }
    Connections {
        target: Native.Hyprland.toplevels
        function onValuesChanged() {
            root.scheduleReconcile();
        }
    }
    Connections {
        target: Native.Hyprland
        function onUsingLuaChanged() {
            if (Native.Hyprland.usingLua)
                root.reload();
        }
        function onRawEvent(event) {
            const name = event.name;
            const kinds = {
                activewindowv2: "window-focus",
                urgent: "window-urgent",
                openwindow: "window-open",
                closewindow: "window-close",
                movewindow: "window-workspace",
                windowtitle: "window-title",
                windowtitlev2: "window-title",
                movewindowv2: "window-workspace",
                changefloatingmode: "window-floating",
                fullscreen: "window-fullscreen",
                pin: "window-pin",
                configreloaded: "configuration-reloaded"
            };
            if (name === "configreloaded")
                root.configurationReloaded();
            if (name === "custom")
                root.panelGesture(event.data);
            if (name === "activewindowv2") {
                root.focusRevision++;
                root.observeFocus(event.data);
            }
            if (name === "urgent")
                root.observeUrgency(event.data);
            if (name === "closewindow")
                root.clearUrgency(event.data);
            if (/^(workspace|focusedmon|monitoradded|monitorremoved|activespecial|moveworkspace)/.test(name))
                root.monitorRefreshPending = true;
            if (/^(openwindow|closewindow|movewindow|activewindow|changefloatingmode|fullscreen|windowtitle|urgent|pin|changegroup|togglegroup|moveintogroup|moveoutofgroup|monitoradded|monitorremoved|workspace|focusedmon|activespecial|moveworkspace)/.test(name))
                metadata.restart();
            if (kinds[name])
                root.event({
                    type: kinds[name],
                    windowId: name === "fullscreen" ? root.activeClient?.windowId || "" : root.addressOf(String(event.data).split(",")[0])
                });
            else if (/^(workspace|focusedmon|monitoradded|monitorremoved|activespecial|moveworkspace)/.test(name))
                root.event({
                    type: "workspace-monitor-change"
                });
        }
    }
    Timer {
        id: metadata
        objectName: "compositorRefreshDelay"
        interval: 50
        onTriggered: {
            if (root.monitorRefreshPending) {
                root.monitorRefreshPending = false;
                Native.Hyprland.refreshMonitors();
                Native.Hyprland.refreshWorkspaces();
            }
            root.refreshToplevels();
        }
    }
    Process {
        id: cursor
        command: ["hyprctl", "cursorpos", "-j"]
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    const point = JSON.parse(text);
                    if (Number.isFinite(point.x) && Number.isFinite(point.y))
                        root.cursorPos = Qt.point(point.x, point.y);
                } catch (failure) {}
            }
        }
    }
    Process {
        id: focusReader
        property int generation: 0
        command: ["hyprctl", "activewindow", "-j"]
        onStarted: generation = root.focusRevision
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    const window = JSON.parse(text);
                    if (focusReader.generation === root.focusRevision)
                        root.observeFocus(window.address);
                } catch (failure) {}
            }
        }
    }
    IpcHandler {
        target: "compositorState"
        function state(): string {
            return JSON.stringify({
                clients: root.clients.length,
                workspace: root.activeWsId,
                focusedMonitor: root.focusedMonitor?.name || "",
                activeKnown: !!root.activeClient,
                refreshCount: root.refreshCount,
                usingLua: Native.Hyprland.usingLua,
                error: root.error
            });
        }
    }
}
