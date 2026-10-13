pragma Singleton
import Quickshell
import Quickshell.Io
import QtQuick
import qs.utils

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
    property var queue: []
    readonly property bool busy: worker.running
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
        if (!worker.running && queue.length) {
            worker.change = queue[0];
            queue = queue.slice(1);
            worker.command = ["python3", NacrePaths.tools + "/desktop-settings.py", ...worker.change.arguments];
            worker.running = true;
        }
    }
    function set(key, value) {
        if (JSON.stringify(data[key]) !== JSON.stringify(value))
            request(["set", key, JSON.stringify(value)]);
    }
    function accept(text) {
        try {
            const result = JSON.parse(text);
            if (!result.data || typeof result.data !== "object")
                throw new Error("Invalid settings response");
            persistedData = result.data;
            monitors = result.monitors ?? monitors;
            pending = result.pending ?? false;
            message = result.error ?? result.message ?? "";
        } catch (e) {
            message = NacreI18n.tr("ui.desktop-settings.message.could-not-read-desktop-settings", "Could not read desktop settings");
        }
    }
    function finish(code) {
        const change = worker.change;
        if (change?.key && desired[change.key]?.revision === change.revision) {
            const values = Object.assign({}, desired);
            delete values[change.key];
            desired = values;
        }
        if (code !== 0 && !message)
            message = NacreI18n.tr("ui.desktop-settings.message.could-not-save-desktop-settings", "Could not save desktop settings");
        next();
    }
    function refresh() {
        if (!busy && queue.length === 0)
            request(["state"]);
    }
    Component.onCompleted: request(["state"])
    FileView {
        path: NacrePaths.config + "/desktop.json"
        watchChanges: true
        onFileChanged: reload()
        onLoaded: {
            try {
                root.persistedData = JSON.parse(text());
            } catch (e) {
                console.warn("DesktopSettings: Cannot decode persisted desktop settings:", e);
            }
        }
    }
    readonly property bool visibleSettings: NacrePanelState.settingsVisible
    onVisibleSettingsChanged: if (visibleSettings)
        refresh()
    Timer {
        objectName: "settingsRefreshTimer"
        interval: 3000
        repeat: true
        running: root.visibleSettings
        onTriggered: root.refresh()
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
