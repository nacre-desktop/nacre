pragma Singleton
import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
import qs.modules.pearls as Pearl

Singleton {
    id: root
    property bool restored: false
    property bool restoring: false
    property var pendingLayout: null
    property int serial: 0
    property var entries: []
    property var views: []
    property var drag: null
    property var lastGesture: ({})
    property string error: ""
    readonly property bool enabled: DesktopSettings.data.detachablePanels !== false
    Component {
        id: factory
        Pearl.NacrePearlWindow {}
    }
    Component {
        id: entryFactory
        QtObject {
            property string key
            property string kind
            property string title
            property string origin
            property var initial: ({})
            property var chat: null
            property var window: null
            property var providerView: null
        }
    }
    function saveSoon() {
        if (restored && !restoring)
            saveDelay.restart();
    }
    function snapshot() {
        return {
            version: 1,
            ai: NacreAiViews.snapshot(),
            windows: entries.map(entry => ({
                        kind: entry.kind,
                        origin: entry.window.screen?.name || entry.origin,
                        initial: entry.window.captureState(),
                        placement: entry.window.placement(),
                        channel: entry.chat?.channel || ""
                    }))
        };
    }
    function restoreLayout(value) {
        if (value.error) {
            error = "Panel layout could not be restored; existing file preserved";
            return;
        }
        restoring = true;
        if (NacreProfile.ai)
            NacreAiViews.restore(value.ai || {});
        for (const data of value.windows || []) {
            if (data.kind === "ai" && !NacreProfile.ai)
                continue;
            const model = data.kind === "ai" ? NacreAiViews.sessions.find(item => item.channel === data.channel) : null;
            if (data.kind === "ai" && (!model || model === NacreAiViews.edge))
                continue;
            if (model) {
                NacreAiViews.parked = NacreAiViews.parked.filter(item => item !== model);
                model.parked = false;
            }
            open(data.kind, data.initial || {}, data.origin || "", data.placement || null, model);
        }
        restoring = false;
        restored = true;
    }
    Timer {
        id: saveDelay
        interval: 300
        onTriggered: {
            root.pendingLayout = root.snapshot();
            root.writeLayout();
        }
    }
    function writeLayout() {
        if (layoutWriter.running || !pendingLayout)
            return;
        layoutWriter.value = pendingLayout;
        pendingLayout = null;
        layoutWriter.running = true;
    }
    Process {
        id: layoutReader
        running: NacreProfile.ready
        command: ["python3", (Quickshell.env("NACRE_PEARL_TOOLS") || Quickshell.env("HOME") + "/.local/share/nacre/shell/tools") + "/pearl-state.py", "load"]
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    root.restoreLayout(JSON.parse(text));
                } catch (e) {
                    root.error = "Could not read saved panel layout";
                }
            }
        }
    }
    Process {
        id: layoutWriter
        property var value
        stdinEnabled: true
        command: ["python3", (Quickshell.env("NACRE_PEARL_TOOLS") || Quickshell.env("HOME") + "/.local/share/nacre/shell/tools") + "/pearl-state.py", "save"]
        onStarted: write(JSON.stringify(value) + "\n")
        onExited: (exitCode, exitStatus) => {
            if (exitCode !== 0)
                root.error = "Could not save panel layout";
            root.writeLayout();
        }
    }
    function edge(kind) {
        return ({
                dashboard: "top",
                media: "top",
                launcher: "bottom",
                ai: "left",
                controls: "right"
            })[kind] || "";
    }
    function screen(name) {
        return Quickshell.screens.find(item => item.name === name) || Quickshell.screens.find(item => item.name === NacrePanelState.activeName()) || Quickshell.screens[0];
    }
    function monitors() {
        return Quickshell.screens.map(item => ({
                    name: item.name,
                    x: item.x,
                    y: item.y,
                    width: item.width,
                    height: item.height
                }));
    }
    function open(kind, initial = {}, origin = "", point = null, chat = null) {
        if (!enabled || !["launcher", "dashboard", "ai", "controls", "media"].includes(kind))
            return null;
        const existing = entries.find(entry => kind === "controls" ? entry.kind === kind : kind === "ai" && chat ? entry.kind === kind && entry.chat === chat : false);
        if (existing) {
            existing.window.activate();
            return existing;
        }
        serial++;
        const entry = entryFactory.createObject(root, {
            key: Date.now().toString(36) + "-" + serial,
            kind: kind,
            title: "Nacre Pearl " + ({
                    launcher: "Launcher",
                    dashboard: "Dashboard",
                    ai: "AI",
                    controls: "Controls",
                    media: "Media"
                })[kind] + " " + serial,
            origin: origin || NacrePanelState.activeName(),
            initial: initial,
            chat: chat
        });
        entries = [...entries, entry];
        const win = factory.createObject(root, {
            entry: entry,
            dropPoint: point
        });
        if (!win) {
            entries = entries.filter(value => value !== entry);
            error = "Could not open panel window";
            entry.destroy();
            return null;
        }
        entry.window = win;
        entry.providerView = win.providerView;
        views = entries.map(value => value.providerView).filter(Boolean);
        saveSoon();
        return entry;
    }
    function detach(kind, host, name, point = null, deferClose = false) {
        if (!enabled)
            return null;
        const view = NacrePanelState.screens[name];
        if (!view)
            return null;
        if (kind === "launcher" && host?.canDetach === false)
            return null;
        if (!deferClose && host?.prepareLift)
            host.prepareLift();
        const data = host?.captureState ? host.captureState() : kind === "dashboard" ? {
            dashboardTab: view.dashboardTab
        } : {
            section: host?.section || view.controlSection || "home"
        };
        const model = kind === "ai" ? NacreAiViews.edge : null;
        const entry = open(kind, data, name, point || Object.assign(bodyRect(host, name), {
            exact: true
        }), model);
        if (!entry)
            return null;
        if (!deferClose) {
            drag = {
                kind: kind,
                host: host,
                screen: name,
                body: bodyRect(host, name),
                dx: 0,
                dy: 0,
                ready: true,
                released: true,
                destination: entry
            };
            handoffTimeout.restart();
        }
        return entry;
    }
    function finishEdge(kind, name, model) {
        const view = NacrePanelState.screens[name];
        if (!view)
            return;
        NacreHoverIntent.dismiss(screen(name));
        if (kind === "ai" && NacreAiViews.edge === model)
            NacreAiViews.take();
        const field = ({
                ai: "left",
                controls: "osd",
                dashboard: "dashboard",
                launcher: "launcher"
            })[kind];
        view[field] = false;
        view.edgeMenu = "";
        if (kind === "ai")
            view.leftPinned = false;
        if (kind === "dashboard")
            view.dashboardTab = 0;
        if (kind === "launcher") {
            view.launcherQuery = "";
            view.launcherRoute = "home";
        }
    }
    function remove(entry, park = true) {
        if (!entries.includes(entry))
            return;
        if (park && entry.kind === "ai")
            NacreAiViews.park(entry.chat);
        entries = entries.filter(value => value !== entry);
        views = entries.map(value => value.providerView).filter(Boolean);
        entry.window.visible = false;
        saveSoon();
        const window = entry.window;
        Qt.callLater(() => window.destroy());
        Qt.callLater(() => entry.destroy());
    }
    function dock(entry, destination = "") {
        const name = destination || entry.window.screen?.name || entry.origin;
        const view = NacrePanelState.screens[name] || NacrePanelState.getForActive();
        if (!view) {
            error = "No desktop edge is available";
            return;
        }
        const data = entry.window.captureState();
        if (entry.kind === "ai") {
            NacreAiViews.dock(entry.chat);
            NacrePanelState.openEdge("left", name);
            Qt.callLater(() => NacrePanelState.panels[name]?.leftDrawer?.restoreState(data));
        }
        if (entry.kind === "dashboard" || entry.kind === "media") {
            view.dashboardTab = entry.kind === "media" ? 2 : data.dashboardTab || 0;
            NacrePanelState.openEdge("dashboard", name);
        }
        if (entry.kind === "controls")
            NacrePanelState.openControls(data.section || "home", name, true);
        if (entry.kind === "launcher") {
            NacrePanelState.openMode(data.mode || "apps", data.query || "", false, name, true);
            view.launcherRoute = data.route || "home";
            const panel = NacrePanelState.panels[name]?.launcher;
            Qt.callLater(() => panel?.restoreState(data));
        }
        remove(entry, false);
    }
    function bodyRect(host, name) {
        const monitor = screen(name);
        const point = host.mapToItem(host.Window.window.contentItem, 0, 0);
        return {
            x: monitor.x + point.x,
            y: monitor.y + point.y,
            width: host.width,
            height: host.height
        };
    }
    function begin(kind, host, name, point, entry = null) {
        if (!enabled || DesktopSettings.data.dragPanels === false || drag || host?.canDetach === false)
            return;
        lastGesture = {
            stage: "begin",
            kind: kind
        };
        const sensorBody = bodyRect(host, name);
        if (host.prepareLift)
            host.prepareLift();
        drag = {
            kind: kind,
            host: host,
            screen: name,
            start: point,
            point: point,
            entry: entry,
            body: bodyRect(host, name),
            sensorBody: sensorBody,
            dx: 0,
            dy: 0,
            ready: false,
            released: false,
            destination: null
        };
        const view = NacrePanelState.screens[name];
        if (view)
            view.edgeMenu = ({
                    ai: "left",
                    controls: "osd",
                    dashboard: "dashboard",
                    launcher: "launcher"
                })[kind];
    }
    function update(point) {
        if (!drag || drag.released)
            return;
        const dx = point.x - drag.start.x, dy = point.y - drag.start.y;
        drag = Object.assign({}, drag, {
            point: point,
            dx: dx,
            dy: dy,
            ready: Math.hypot(dx, dy) >= 16
        });
    }
    function finish() {
        if (!drag || drag.released)
            return;
        lastGesture = {
            stage: "release",
            kind: drag.kind,
            dx: drag.dx,
            dy: drag.dy,
            ready: drag.ready
        };
        if (!drag.ready) {
            drag = Object.assign({}, drag, {
                dx: 0,
                dy: 0,
                released: true
            });
            returnDelay.restart();
            return;
        }
        const value = drag;
        const point = Object.assign({}, value.body, {
            x: value.body.x + value.dx,
            y: value.body.y + value.dy,
            exact: true
        });
        drag = Object.assign({}, value, {
            released: true
        });
        const entry = detach(value.kind, value.host, value.screen, point, true);
        if (!entry) {
            cancel();
            return;
        }
        drag = Object.assign({}, drag, {
            destination: entry
        });
        handoffTimeout.restart();
    }
    function handoffFailed(entry) {
        if (drag?.destination === entry) {
            error = "Panel could not be positioned; it remains attached";
            cancel();
            remove(entry, false);
        }
    }
    function handedOff(entry) {
        if (drag?.destination === entry) {
            drag.host.handoffHidden = true;
            finishEdge(drag.kind, drag.screen, entry.chat);
            cancel();
            // Activate after the layer releases its exclusive keyboard focus.
            Qt.callLater(() => entry.window.activate());
        }
    }
    function cancel() {
        if (drag && !drag.released)
            lastGesture = {
                stage: "cancel",
                kind: drag.kind,
                dx: drag.dx,
                dy: drag.dy
            };
        drag = null;
    }
    Timer {
        id: returnDelay
        interval: 220
        onTriggered: root.cancel()
    }
    Timer {
        id: handoffTimeout
        interval: 5000
        onTriggered: {
            // A failed compositor placement must not strand the original panel.
            const entry = root.drag?.destination;
            root.cancel();
            if (entry && !entry.window.ready)
                root.remove(entry, false);
        }
    }
    function detachActive() {
        const name = NacrePanelState.activeName(), view = NacrePanelState.screens[name], panels = NacrePanelState.panels[name];
        if (!view || !panels)
            return;
        const kind = view.launcher ? "launcher" : view.dashboard ? "dashboard" : view.osd ? "controls" : view.left ? "ai" : "";
        if (kind)
            detach(kind, ({
                    launcher: panels.launcher,
                    dashboard: panels.dashboard,
                    controls: panels.osd,
                    ai: panels.leftDrawer
                })[kind], name);
    }
    IpcHandler {
        target: "pearls"
        function open(kind: string): void {
            if (!root.enabled)
                return;
            const model = kind === "ai" ? NacreAiViews.edge : null;
            const entry = root.open(kind, {}, NacrePanelState.activeName(), null, model);
            if (entry && model && NacreAiViews.edge === model)
                NacreAiViews.take();
        }
        function detach(): void {
            root.detachActive();
        }
        function dock(key: string): void {
            const entry = root.entries.find(value => value.key === key);
            if (entry)
                root.dock(entry);
        }
        function close(key: string): void {
            const entry = root.entries.find(value => value.key === key);
            if (entry)
                root.remove(entry);
        }
        function gesture(): string {
            return JSON.stringify(root.lastGesture);
        }
        function state(): string {
            return JSON.stringify(root.entries.map(value => ({
                        key: value.key,
                        kind: value.kind,
                        title: value.title,
                        visible: value.window?.visible,
                        tab: value.window?.viewState.dashboardTab,
                        route: value.window?.viewState.launcherRoute
                    })));
        }
    }
}
