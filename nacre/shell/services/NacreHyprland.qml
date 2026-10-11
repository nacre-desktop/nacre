pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Hyprland as Native

Singleton {
    id: root
    property var clients: []
    property var byAddress: ({})
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
    Component.onCompleted: {
        reconcile();
        if (!cursor.running)
            cursor.running = true;
        focusReader.running = true;
    }
    Component {
        id: factory
        NacreClient {
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
            if (name === "activewindowv2") {
                root.focusRevision++;
                root.observeFocus(event.data);
            }
            if (/^(workspace|focusedmon|monitoradded|monitorremoved|activespecial|moveworkspace)/.test(name))
                root.monitorRefreshPending = true;
            if (/^(openwindow|closewindow|movewindow|activewindow|changefloatingmode|fullscreen|windowtitle|urgent|pin|changegroup|togglegroup|moveintogroup|moveoutofgroup|monitoradded|monitorremoved|workspace|focusedmon|activespecial|moveworkspace)/.test(name))
                metadata.restart();
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
