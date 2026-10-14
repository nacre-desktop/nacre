pragma Singleton
import QtQuick
import qs.utils
import Quickshell
import Quickshell.Io

Singleton {
    id: root
    property var preferences: ({
            tabs: ["today", "media", "weather", "system"],
            lyrics: true,
            albumPalette: true,
            focusMinutes: 25
        })
    property var focusState: ({})
    property var calendar: ({
            events: []
        })
    property var brain: ({
            available: false,
            recent: [],
            tasks: []
        })
    property var home: ({})
    property string message: ""
    property var queue: []
    property real now: Date.now() / 1000
    property bool finishing: false
    property string activeAction: ""
    property bool restoreDndPending: false
    property bool restoreDndValue: false
    property bool completionPending: false
    property bool noticePending: false
    property bool focusWriteFailed: false
    property bool enablingDnd: false
    readonly property bool busy: worker.running
    property var enabledExtras: []
    property var extraOrder: NacreDashboardRegistry.extras.map(page => page.id)
    property bool extrasReady: false
    property bool weatherPhotos: false
    readonly property var coreTabs: {
        const saved = preferences.tabs || [];
        const legacy = JSON.stringify(saved) === JSON.stringify(["today", "weather", "media", "system", "focus"]);
        const core = legacy || !saved.length ? NacreDashboardRegistry.core : saved.filter(id => NacreDashboardRegistry.core.includes(id));
        return core.length ? core : ["today"];
    }
    readonly property var tabs: [...coreTabs, ...extraOrder.filter(id => enabledExtras.includes(id))]
    function restoreExtras(text) {
        try {
            const state = JSON.parse(text), known = NacreDashboardRegistry.extras.map(page => page.id);
            if (state.version !== 1 || !Array.isArray(state.enabled) || !Array.isArray(state.order))
                throw new Error();
            enabledExtras = [...new Set(state.enabled.filter(id => known.includes(id)))];
            extraOrder = [...new Set([...state.order.filter(id => known.includes(id)), ...known])];
            weatherPhotos = state.weatherPhotos === true;
            extrasReady = true;
        } catch (error) {
            message = NacreI18n.tr("dashboard.state.extras-read-error", "Optional pages could not be read.");
        }
    }
    function saveExtras() {
        if (extrasReady)
            extraFile.setText(JSON.stringify({
                version: 1,
                enabled: enabledExtras,
                order: extraOrder,
                weatherPhotos: weatherPhotos
            }));
    }
    function toggleExtra(id) {
        if (!NacreDashboardRegistry.entry(id) || !extrasReady)
            return;
        enabledExtras = enabledExtras.includes(id) ? enabledExtras.filter(value => value !== id) : [...enabledExtras, id];
        saveExtras();
    }
    function reorderExtra(id, delta) {
        const items = [...extraOrder], before = items.indexOf(id), after = Math.max(0, Math.min(items.length - 1, before + delta));
        if (before < 0 || before === after)
            return;
        items.splice(before, 1);
        items.splice(after, 0, id);
        extraOrder = items;
        saveExtras();
    }
    FileView {
        id: extraFile
        path: NacrePaths.config + "/dashboard-extras.json"
        atomicWrites: true
        watchChanges: true
        printErrors: false
        onLoaded: root.restoreExtras(text())
        onFileChanged: reload()
        onLoadFailed: error => {
            if (error === FileViewError.FileNotFound)
                root.extrasReady = true;
        }
        onSaveFailed: root.message = NacreI18n.tr("dashboard.state.extras-save-error", "Optional pages could not be saved.")
    }
    readonly property bool shown: NacrePanelState.providerViews.some(view => (view.detached || NacrePanelState.hidden !== true) && (view.dashboard))
    readonly property bool brainShown: NacrePanelState.providerViews.some(view => (view.detached || NacrePanelState.hidden !== true) && view.dashboard && view.dashboardTab === 11)
    readonly property bool focusShown: NacrePanelState.providerViews.some(view => (view.detached || NacrePanelState.hidden !== true) && (view.dashboard && view.dashboardTab === 4))
    readonly property bool running: focusState.status === "running"
    readonly property real remaining: running ? Math.max(0, (focusState.deadline || 0) - now) : focusState.status === "paused" ? focusState.remaining || 0 : 0
    readonly property real progress: ["running", "paused"].includes(focusState.status) && focusState.total > 0 ? Math.max(0, Math.min(1, 1 - remaining / focusState.total)) : 0
    readonly property string timerText: Math.floor(remaining / 60).toString().padStart(2, "0") + ":" + Math.floor(remaining % 60).toString().padStart(2, "0")
    function request(action, value = "") {
        queue.push([action, value]);
        next();
    }
    function next() {
        if (worker.running || !queue.length)
            return;
        const item = queue.shift();
        activeAction = item[0];
        worker.command = [NacrePaths.helperBinary, "dashboard-data", ...item];
        worker.running = true;
    }
    function refresh() {
        if (!worker.running && !queue.length)
            request(brainShown ? "context" : "read");
    }
    function saveMany(values) {
        preferences = Object.assign({}, preferences, values);
        request("preferences", JSON.stringify(preferences));
    }
    function save(key, value) {
        saveMany({
            [key]: value
        });
    }
    function reorder(name, delta) {
        const values = [...coreTabs], index = values.indexOf(name), nextIndex = Math.max(0, Math.min(values.length - 1, index + delta));
        if (index < 0 || ![-1, 1].includes(delta))
            return;
        values.splice(index, 1);
        values.splice(nextIndex, 0, name);
        save("tabs", values);
    }
    function toggleTab(name) {
        const values = coreTabs.includes(name) ? coreTabs.filter(value => value !== name) : [...coreTabs, name];
        if (values.length)
            save("tabs", values);
    }
    function start(minutes) {
        if (worker.running && activeAction === "focus-start" || queue.some(item => item[0] === "focus-start"))
            return;
        const previousDnd = DesktopSettings.data.dnd === true;
        request("focus-start", JSON.stringify({
            minutes: minutes,
            dnd: previousDnd
        }));
    }
    function end(completed = false) {
        if (finishing || !["running", "paused"].includes(focusState.status))
            return;
        finishing = true;
        restoreDndPending = focusState.ownsDnd && DesktopSettings.data.dnd === true;
        restoreDndValue = focusState.previousDnd === true;
        completionPending = completed;
        request(completed ? "focus-complete" : "focus-stop");
    }
    function notifyComplete() {
        if (!noticePending)
            return;
        noticePending = false;
        noticeFallback.stop();
        Quickshell.execDetached(["notify-send", "--app-name=Nacre Focus", "--icon=nacre-notifications", NacreI18n.tr("focus.complete.title", "Focus complete"), NacreI18n.tr("focus.complete.body", "Take a moment to rest.")]);
    }
    function failed(message) {
        root.message = message;
        if (root.activeAction.startsWith("focus-")) {
            root.focusWriteFailed = true;
            root.focusState = Object.assign({}, root.focusState, {
                status: "paused",
                remaining: root.remaining
            });
        }
        root.finishing = false;
        root.completionPending = false;
    }
    function publish(text) {
        try {
            const data = JSON.parse(text);
            if (data.error) {
                root.failed(data.error);
                return;
            }
            if (!data.preferences || !Array.isArray(data.preferences.tabs) || !data.focus || !data.calendar || !data.home)
                throw new Error();
            root.preferences = data.preferences;
            if (!root.focusWriteFailed || root.activeAction.startsWith("focus-"))
                root.focusState = data.focus;
            if (root.activeAction.startsWith("focus-"))
                root.focusWriteFailed = false;
            root.now = Date.now() / 1000;
            root.calendar = data.calendar;
            root.home = data.home;
            if (data.brain?.available !== undefined)
                root.brain = data.brain;
            root.message = root.focusWriteFailed ? root.message : "";
            if (root.activeAction === "focus-start" && root.focusState.ownsDnd) {
                if (DesktopSettings.data.dnd === false) {
                    root.enablingDnd = true;
                    DesktopSettings.set("dnd", true);
                } else
                    root.request("focus-release");
            }
            if (root.finishing) {
                root.noticePending = root.completionPending && root.focusState.status === "done";
                if (root.restoreDndPending && DesktopSettings.data.dnd === true) {
                    DesktopSettings.set("dnd", root.restoreDndValue);
                    if (root.noticePending)
                        noticeFallback.restart();
                } else
                    root.notifyComplete();
                root.restoreDndPending = false;
            }
            root.finishing = false;
            root.completionPending = false;
        } catch (error) {
            root.failed(NacreI18n.tr("ui.dashboard-state.description.dashboard-data-unavailable", "Dashboard data unavailable"));
        }
    }
    function openNote(path) {
        if (typeof path === "string" && path.startsWith("/"))
            Quickshell.execDetached(["xdg-open", path]);
    }
    onShownChanged: if (shown)
        Qt.callLater(root.refresh)
    onBrainShownChanged: if (brainShown)
        request("context")
    onFocusShownChanged: if (focusShown)
        request("context")
    Component.onCompleted: refresh()
    Connections {
        target: DesktopSettings
        function onDataChanged() {
            if (root.enablingDnd && DesktopSettings.data.dnd === true)
                root.enablingDnd = false;
            if (root.finishing && DesktopSettings.data.dnd === false)
                root.restoreDndPending = false;
            if (root.noticePending && DesktopSettings.data.dnd === root.restoreDndValue)
                root.notifyComplete();
            if (root.focusState.ownsDnd && DesktopSettings.data.dnd === false && !root.finishing && !root.enablingDnd)
                root.request("focus-release");
        }
    }
    FileView {
        path: NacrePaths.config + "/dashboard.json"
        watchChanges: true
        printErrors: false
        onFileChanged: root.refresh()
    }
    Timer {
        id: noticeFallback
        interval: 1000
        onTriggered: root.notifyComplete()
    }
    Timer {
        interval: 1000
        repeat: true
        running: root.running
        onTriggered: {
            root.now = Date.now() / 1000;
            if (root.remaining <= 0)
                root.end(true);
        }
    }
    Timer {
        interval: 60000
        repeat: true
        running: root.shown
        onTriggered: root.refresh()
    }
    NacreHelperTask {
        id: worker
        onOutput: text => root.publish(text)
        onExited: Qt.callLater(root.next)
    }
    IpcHandler {
        target: "dashboardState"
        function state(): string {
            return JSON.stringify({
                tabs: root.tabs,
                running: root.running,
                remaining: root.remaining,
                progress: root.progress,
                shown: root.shown,
                message: root.message,
                busy: worker.running,
                queued: root.queue.length,
                focusShown: root.focusShown,
                brainAvailable: root.brain.available
            });
        }
    }
}
