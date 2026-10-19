pragma Singleton
import Quickshell
import Quickshell.Io
import QtQuick
import qs.utils
import "daemon-state.js" as State

Singleton {
    id: root
    property var persistedData: ({})
    property var desired: ({})
    property int revision: 0
    readonly property var data: {
        const values = Object.assign({}, persistedData);
        for (const key of Object.keys(desired))
            values[key] = desired[key].value;
        return values;
    }
    property var monitors: []
    property bool pending: false
    property string message: ""
    property string error: ""
    property var queue: []
    readonly property bool daemonMode: NacreDaemon.enabled
    property bool daemonBusy: false
    property double remoteRevision: -1
    readonly property var daemonState: NacreDaemon.topic("settings.desktop")
    readonly property var daemonStore: NacreDaemon.topic("settings.store.desktop")
    property bool daemonSubscribed: false
    property bool externalWriteBlocked: false
    onExternalWriteBlockedChanged: if (!externalWriteBlocked)
        next()
    readonly property bool busy: worker.running || daemonBusy
    function request(args) {
        const change = {
            arguments: args
        };
        const pending = queue.slice();
        if (args[0] === "set" && args.length === 3) {
            change.key = args[1];
            change.value = JSON.parse(args[2]);
            change.revision = ++revision;
            desired = Object.assign({}, desired, {
                [change.key]: {
                    value: change.value,
                    revision: change.revision
                }
            });
            let found = -1;
            for (let index = pending.length - 1; index >= 0 && pending[index].arguments[0] === "set"; index--)
                if (pending[index].key === change.key) {
                    found = index;
                    break;
                }
            if (found >= 0)
                pending.splice(found, 1);
        }
        pending.push(change);
        queue = pending;
        next();
    }
    function next() {
        if (!busy && !externalWriteBlocked && queue.length) {
            if (daemonMode) {
                if (!NacreDaemon.connected)
                    return;
                daemonBusy = true;
                if (remoteRevision < 0 && queue[0].arguments[0] !== "state") {
                    NacreDaemon.call("settings.get", {
                        store: "desktop"
                    }, (result, failure) => {
                        if (failure) {
                            error = failure.message;
                            message = error;
                            worker.change = queue[0];
                            queue = queue.slice(1);
                            daemonBusy = false;
                            finish(1);
                        } else {
                            remoteRevision = result.rev;
                            daemonBusy = false;
                            next();
                        }
                    });
                    return;
                }
                worker.change = queue[0];
                queue = queue.slice(1);
                const params = {
                    arguments: worker.change.arguments
                };
                if (remoteRevision >= 0)
                    params.rev = remoteRevision;
                NacreDaemon.call("settings.desktop", params, (result, failure) => {
                    if (failure) {
                        error = failure.message;
                        message = error;
                    } else {
                        remoteRevision = result.rev;
                        acceptData(result);
                    }
                    daemonBusy = false;
                    finish(failure ? 1 : 0);
                }, 30000);
                return;
            }
            worker.change = queue[0];
            queue = queue.slice(1);
            const hostTools = NacrePaths.hostTools || NacrePaths.tools;
            const environment = hostTools !== NacrePaths.tools ? ["NACRE_NIRI_TOOLS_SOURCE=" + hostTools, "NACRE_HELPER_BINARY=" + hostTools + "/nacre-helper"] : [];
            if (NacrePaths.niriDesktopConfig)
                environment.push("NACRE_NIRI_DESKTOP_CONFIG=" + NacrePaths.niriDesktopConfig);
            worker.command = [...(environment.length ? ["env", ...environment] : []), NacrePaths.helperBinary, "desktop-settings", ...worker.change.arguments];
            worker.running = true;
        }
    }
    function set(key, value) {
        if (JSON.stringify(data[key]) !== JSON.stringify(value))
            request(["set", key, JSON.stringify(value)]);
    }
    function accept(text) {
        try {
            acceptData(JSON.parse(text));
        } catch (e) {
            error = NacreI18n.tr("ui.desktop-settings.message.could-not-read-desktop-settings", "Could not read desktop settings");
            message = error;
        }
    }
    function acceptData(result) {
        try {
            if (!result.data || typeof result.data !== "object")
                throw new Error("Invalid settings response");
            if (!State.equal(persistedData, result.data))
                persistedData = State.copy(result.data);
            if (result.monitors && !State.equal(monitors, result.monitors))
                monitors = State.copy(result.monitors);
            pending = result.pending ?? false;
            error = result.error || "";
            message = error || result.message || "";
        } catch (e) {
            error = NacreI18n.tr("ui.desktop-settings.message.could-not-read-desktop-settings", "Could not read desktop settings");
            message = error;
        }
    }
    function finish(code) {
        const change = worker.change;
        if (change?.key && desired[change.key]?.revision === change.revision) {
            const values = Object.assign({}, desired);
            delete values[change.key];
            desired = values;
        }
        if (code !== 0 && !error) {
            error = NacreI18n.tr("ui.desktop-settings.message.could-not-save-desktop-settings", "Could not save desktop settings");
            message = error;
        }
        next();
    }
    function refresh() {
        if (!queue.some(item => item.arguments[0] === "state"))
            request(["state"]);
    }
    function subscribeDaemon() {
        if (daemonMode && !daemonSubscribed) {
            daemonSubscribed = true;
            NacreDaemon.subscribe("settings.desktop");
            NacreDaemon.subscribe("settings.store.desktop");
        } else if (!daemonMode && daemonSubscribed) {
            daemonSubscribed = false;
            NacreDaemon.unsubscribe("settings.desktop");
            NacreDaemon.unsubscribe("settings.store.desktop");
        }
        next();
    }
    onDaemonModeChanged: subscribeDaemon()
    Component.onCompleted: {
        subscribeDaemon();
        request(["state"]);
    }
    Component.onDestruction: if (daemonSubscribed) {
        NacreDaemon.unsubscribe("settings.desktop");
        NacreDaemon.unsubscribe("settings.store.desktop");
    }
    Connections {
        target: root.daemonState
        function onDataChanged() {
            if (root.daemonMode && root.daemonState.data.available)
                root.acceptData(root.daemonState.data);
        }
    }
    Connections {
        target: root.daemonStore
        function onDataChanged() {
            const snapshot = root.daemonStore.data;
            if (!root.daemonMode || snapshot.store !== "desktop")
                return;
            const changed = root.remoteRevision !== snapshot.rev;
            root.remoteRevision = snapshot.rev;
            if (snapshot.error) {
                root.error = snapshot.error.message;
                root.message = root.error;
            } else if (changed && !root.daemonBusy) {
                root.refresh();
            }
        }
    }
    Connections {
        target: NacreDaemon
        function onNewBoot() {
            root.remoteRevision = -1;
            root.refresh();
        }
        function onConnectedChanged() {
            if (NacreDaemon.connected) {
                root.refresh();
                root.next();
            } else {
                root.remoteRevision = -1;
            }
        }
    }
    Loader {
        active: !root.daemonMode
        sourceComponent: Component {
            FileView {
                printErrors: false
                onLoadFailed: error => {
                    if (error !== FileViewError.FileNotFound)
                        console.warn("Desktop settings could not be read (" + error + ")");
                }
                path: NacrePaths.config + "/desktop.json"
                watchChanges: true
                onFileChanged: reload()
                onLoaded: {
                    try {
                        const data = JSON.parse(text());
                        if ((data.version || 0) < 2) {
                            const aliases = {
                                topEdge: ["frameTopEnabled", "topBarEnabled"],
                                leftEdge: ["frameLeftEnabled"],
                                rightEdge: ["frameRightEnabled", "controlCenterEnabled"],
                                bottomEdge: ["frameBottomEnabled"],
                                leftDrawer: ["aiSidebarEnabled"]
                            };
                            for (const key of Object.keys(aliases))
                                for (const next of aliases[key])
                                    if (data[next] === undefined && data[key] !== undefined)
                                        data[next] = data[key];
                        }
                        root.persistedData = data;
                    } catch (e) {
                        console.warn("DesktopSettings: Cannot decode persisted desktop settings:", e);
                    }
                }
            }
        }
    }
    readonly property bool visibleSettings: NacrePanelState.settingsVisible
    onVisibleSettingsChanged: if (visibleSettings)
        refresh()
    // Preference writes already arrive through the file watcher. Hardware and
    // display-transaction refreshes are event-driven, not a periodic subprocess.
    // Niri publishes a fresh monitor array with other snapshots too. Compare
    // only physical output properties to avoid spawning a query on focus events.
    readonly property string monitorSignature: JSON.stringify((NacreCompositor.monitors?.values || []).map(monitor => ({
                name: monitor.name,
                width: monitor.width,
                height: monitor.height,
                scale: monitor.scale,
                refreshRate: monitor.refreshRate,
                x: monitor.x,
                y: monitor.y,
                transform: monitor.transform
            })).sort((a, b) => String(a.name).localeCompare(String(b.name))))
    onMonitorSignatureChanged: refresh()
    Loader {
        active: !root.daemonMode
        sourceComponent: Component {
            FileView {
                preload: true
                path: NacrePaths.config + "/display-pending" + (NacreCompositor.backendName === "niri" ? "-niri" : "") + ".json"
                watchChanges: true
                printErrors: false
                onFileChanged: root.refresh()
            }
        }
    }
    Process {
        id: worker
        property var change: null
        stdout: SplitParser {
            splitMarker: ""
            onRead: line => root.accept(line)
        }
        onExited: code => root.finish(code)
    }
    IpcHandler {
        target: "desktopSettings"
        function state(): string {
            return JSON.stringify({
                data: root.data,
                monitors: root.monitors,
                pending: root.pending,
                message: root.message,
                busy: root.busy
            });
        }
        function set(key: string, value: string): void {
            root.set(key, JSON.parse(value));
        }
    }
}
