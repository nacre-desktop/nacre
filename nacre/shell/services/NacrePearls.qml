pragma Singleton
import QtQuick
import qs.utils
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "../modules/pearls/geometry.js" as Geometry
import qs.modules.pearls as Pearl
import qs.config
import qs.widgets

Singleton {
    id: root
    property bool restored: false
    property bool restoring: false
    property var pendingLayout: null
    property int serial: 0
    property var entries: []
    property var views: []
    readonly property var pendingDocks: entries.filter(entry => !!entry.transfer)
    property var drag: null
    property var nativeDrag: null
    property var niriMove: null
    property var lastGesture: ({})
    property string error: ""
    readonly property var magneticCue: !enabled || NacrePanelState.hidden || NacreCompositor.backendName === "niri" ? null : drag && !drag.released ? drag.magnet : nativeDrag?.ready ? nativeDrag.magnet : null
    readonly property var landingCues: {
        if (!enabled || NacrePanelState.hidden)
            return [];
        const kind = drag && !drag.released ? drag.kind : nativeDrag?.ready ? nativeDrag.entry?.kind : niriMove?.entry.kind;
        return kind ? monitors().map(monitor => Geometry.landing(kind, monitor)).filter(Boolean) : [];
    }
    readonly property bool enabled: DesktopSettings.data.detachablePanels !== false
    Component {
        id: factory
        Pearl.NacrePearlWindow {}
    }
    Component {
        id: frameBarrierFactory
        NacreFrameBarrier {}
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
            property var transfer: null
        }
    }
    function nativeGesture(payload) {
        const match = /^nacre_pearl_drag,(begin|move|end),(Nacre Pearl (?:Launcher|Dashboard|AI|Controls|Media) [0-9]+),([^,]+),([^,]+)(?:,([^,]+),([^,]+),([^,]+),([^,]+))?$/.exec(payload);
        if (!match)
            return;
        const entry = entries.find(value => value.title === match[2]);
        const point = {
            x: Number(match[3]),
            y: Number(match[4])
        };
        if (!entry || !Number.isFinite(point.x) || !Number.isFinite(point.y))
            return;
        const first = match[1] === "begin" || nativeDrag?.entry !== entry;
        const start = first ? point : nativeDrag.start;
        const anchor = first ? (match[5] !== undefined ? {
                x: Number(match[5]),
                y: Number(match[6]),
                width: Number(match[7]),
                height: Number(match[8])
            } : entry.window.placement()) : nativeDrag.anchor;
        if (anchor && (![anchor.x, anchor.y, anchor.width, anchor.height].every(Number.isFinite) || anchor.width <= 0 || anchor.height <= 0))
            return;
        const body = anchor ? Object.assign({}, anchor, {
            x: anchor.x + point.x - start.x,
            y: anchor.y + point.y - start.y
        }) : null;
        const ready = Math.hypot(point.x - start.x, point.y - start.y) >= 16;
        const magnet = enabled && !NacrePanelState.hidden && body ? Geometry.magnet(entry.kind, body, point, monitors(), first ? null : nativeDrag.magnet, anchor) : null;
        const destination = ready && magnet?.armed ? magnet : null;
        nativeDrag = {
            entry: entry,
            point: point,
            start: start,
            anchor: anchor,
            body: body,
            ready: ready,
            magnet: magnet,
            destination: destination
        };
        if (match[1] === "end") {
            nativeDrag = null;
            if (destination)
                dock(entry, destination.screen);
            else {
                NacreCompositor.refreshWindows();
                saveSoon();
            }
        }
    }
    function compositorMove(value) {
        if (NacreCompositor.backendName !== "niri" || value?.type !== "window-move")
            return;
        if (value.phase === "cancel") {
            if (!value.windowId || niriMove?.windowId === value.windowId)
                niriMove = null;
            return;
        }
        const client = NacreCompositor.windows.find(item => item.windowId === value.windowId);
        const entry = entries.find(item => item.window?.clientAddress === value.windowId && item.window?.ready && client?.pid === Quickshell.processId && client?.title === item.title);
        if (!entry)
            return;
        if (value.phase === "begin") {
            niriMove = {
                entry: entry,
                windowId: value.windowId
            };
            return;
        }
        if (value.phase !== "end" || niriMove?.entry !== entry)
            return;
        niriMove = null;
        const body = value.body;
        const monitor = monitors().find(item => item.name === value.outputId);
        if (!enabled || NacrePanelState.hidden || value.floating !== true || !monitor || !body || ![body.x, body.y, body.width, body.height].every(Number.isFinite))
            return;
        // Niri publishes the completed move geometry, but no live drag cursor.
        // Select its reported output; the common target still uses panel edges.
        const point = {
            x: Geometry.clamp(body.x + body.width / 2, monitor.x, monitor.x + monitor.width - 1),
            y: Geometry.clamp(body.y + body.height / 2, monitor.y, monitor.y + monitor.height - 1)
        };
        const magnet = Geometry.magnet(entry.kind, body, point, [monitor]);
        if (magnet?.armed) {
            nativeDrag = {
                entry: entry,
                ready: true,
                magnet: magnet
            };
            dock(entry, magnet.screen);
        } else
            saveSoon();
    }
    Connections {
        target: NacreCompositor
        function onEvent(value) {
            root.compositorMove(value);
        }
        function onPanelGesture(data) {
            root.nativeGesture(data);
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
            error = NacreI18n.tr("ui.pearls.error.panel-layout-could-not-be-restored-existing-file-preser", "Panel layout could not be restored; existing file preserved");
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
        command: [NacrePaths.helperBinary, "pearl-state", "load"]
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    root.restoreLayout(JSON.parse(text));
                } catch (e) {
                    root.error = NacreI18n.tr("ui.pearls.error.could-not-read-saved-panel-layout", "Could not read saved panel layout");
                }
            }
        }
    }
    Process {
        id: layoutWriter
        property var value
        stdinEnabled: true
        command: [NacrePaths.helperBinary, "pearl-state", "save"]
        onStarted: write(JSON.stringify(value) + "\n")
        onExited: (exitCode, exitStatus) => {
            if (exitCode !== 0)
                root.error = NacreI18n.tr("ui.pearls.error.could-not-save-panel-layout", "Could not save panel layout");
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
                    height: item.height,
                    frame: {
                        top: NacreFrame.headerHeight,
                        bottom: NacreFrame.bottom,
                        left: NacreFrame.left,
                        right: NacreFrame.right
                    }
                }));
    }
    // Controller identity, never render this metadata directly as a user caption.
    function nativeTitle(kind, serial) {
        return "Nacre Pearl " + ({
                launcher: "Launcher",
                dashboard: "Dashboard",
                ai: "AI",
                controls: "Controls",
                media: "Media"
            })[kind] + " " + serial;
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
            title: nativeTitle(kind, serial),
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
            error = NacreI18n.tr("ui.pearls.error.could-not-open-panel-window", "Could not open panel window");
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
                destination: entry,
                dockTarget: ""
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
        if (entry.transfer) {
            const transfer = entry.transfer;
            entry.transfer = null;
            Qt.callLater(() => transfer.destroy());
        }
        if (nativeDrag?.entry === entry)
            nativeDrag = null;
        if (niriMove?.entry === entry)
            niriMove = null;
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
        if (entry.transfer)
            return;
        const name = destination || entry.window.screen?.name || entry.origin;
        const view = NacrePanelState.screens[name] || NacrePanelState.getForActive();
        if (!view) {
            error = NacreI18n.tr("ui.pearls.error.no-desktop-edge-is-available", "No desktop edge is available");
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
        NacrePanelState.dockAsHover(view);
        // Docking restores an existing panel; it is not a fresh menu opening.
        // Complete the destination before removing the native source surface.
        const panel = NacrePanelState.panels[name]?.[({
                    ai: "leftDrawer",
                    controls: "osd",
                    dashboard: "dashboard",
                    media: "dashboard",
                    launcher: "launcher"
                })[entry.kind]];
        panel?.restoreAttached();
        const surface = panel?.Window?.window;
        if (!surface) {
            rollbackDock(entry, view);
            return;
        }
        entry.transfer = frameBarrierFactory.createObject(root, {
            surface: surface,
            commit: () => remove(entry, false),
            reject: () => rollbackDock(entry, view)
        });
        entries = entries.slice();
        entry.transfer.start();
    }
    function rollbackDock(entry, view) {
        if (!entries.includes(entry))
            return;
        const transfer = entry.transfer;
        entry.transfer = null;
        if (transfer)
            Qt.callLater(() => transfer.destroy());
        const field = ({
                ai: "left",
                controls: "osd",
                dashboard: "dashboard",
                media: "dashboard",
                launcher: "launcher"
            })[entry.kind];
        if (field && view)
            view[field] = false;
        if (entry.kind === "ai" && NacreAiViews.edge === entry.chat)
            NacreAiViews.take();
        entries = entries.slice();
        entry.window.activate?.();
        console.warn("Nacre dock destination did not present; floating window retained");
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
        if (!enabled || DesktopSettings.data.dragPanels === false || host?.canDetach === false)
            return;
        if (drag?.released && !drag.destination) {
            returnDelay.stop();
            cancel();
        }
        if (drag)
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
            destination: null,
            magnet: Geometry.magnet(kind, bodyRect(host, name), point, monitors())
        };
        // Hover exits are suspended by the input controller; do not change the
        // layer's keyboard focus mode while it owns the pointer grab.

    }
    function update(point) {
        if (!drag || drag.released)
            return;
        const dx = point.x - drag.start.x, dy = point.y - drag.start.y;
        drag = Object.assign({}, drag, {
            point: point,
            dx: dx,
            dy: dy,
            ready: Math.hypot(dx, dy) >= 16,
            magnet: Geometry.magnet(drag.kind, Object.assign({}, drag.body, {
                x: drag.body.x + dx,
                y: drag.body.y + dy
            }), point, monitors(), drag.magnet, drag.body)
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
        if (!drag.ready || drag.magnet?.armed && drag.magnet.screen === drag.screen) {
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
            x: Math.round(value.body.x + value.dx),
            y: Math.round(value.body.y + value.dy),
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
            destination: entry,
            dockTarget: value.magnet?.armed ? value.magnet.screen : ""
        });
        handoffTimeout.restart();
        if (entry.window.painted)
            Qt.callLater(() => handedOff(entry));
    }
    function handoffFailed(entry) {
        if (drag?.destination === entry) {
            error = NacreI18n.tr("ui.pearls.error.panel-could-not-be-positioned-it-remains-attached", "Panel could not be positioned; it remains attached");
            cancel();
            remove(entry, false);
        }
    }
    function handedOff(entry) {
        if (entry.window.painted && drag?.destination === entry) {
            const dockTarget = drag.dockTarget;
            drag.host.handoffHidden = true;
            finishEdge(drag.kind, drag.screen, entry.chat);
            cancel();
            // Activate after the layer releases its exclusive keyboard focus.
            Qt.callLater(() => dockTarget ? dock(entry, dockTarget) : entry.window.activate());
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
        if (!kind) {
            const focused = NacreCompositor.activeWindow;
            const entry = entries.find(value => value.title === focused?.title && focused?.pid === Quickshell.processId);
            if (entry)
                dock(entry);
            return;
        }
        if (kind)
            detach(kind, ({
                    launcher: panels.launcher,
                    dashboard: panels.dashboard,
                    controls: panels.osd,
                    ai: panels.leftDrawer
                })[kind], name);
    }
    IpcHandler {
        id: pearlsIpc
        Component.onCompleted: NacreDaemon.registerIpc("pearls", pearlsIpc, {
            "open": ["s"],
            "detach": [],
            "dock": ["s"],
            "close": ["s"],
            "gesture": [],
            "state": []
        })
        Component.onDestruction: NacreDaemon.unregisterIpc("pearls", pearlsIpc)
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
            return JSON.stringify(Object.assign({}, root.lastGesture, {
                error: root.error
            }));
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
