pragma Singleton
import Quickshell
import Quickshell.Io
import QtQuick
import qs.utils
import "../utils/scripts/view-state.js" as Views

Singleton {
    id: root
    property var previous: ({})
    property bool ready: false
    property var pendingNames: []
    property var rebuilding: []
    property var restoringNames: []
    property var saved: ({})
    property string status: NacreI18n.tr("ui.display-recovery.status.ready", "Ready")
    readonly property var surfaceScreens: Quickshell.screens.filter(s => !rebuilding.includes(s.name))
    readonly property string signature: Quickshell.screens.map(s => [s.name, s.x, s.y, s.width, s.height, s.devicePixelRatio].join(":")).sort().join(";")
    function geometry() {
        const values = {};
        for (const s of Quickshell.screens)
            values[s.name] = [s.x, s.y, s.width, s.height, s.devicePixelRatio].join(":");
        return values;
    }
    Component.onCompleted: {
        previous = geometry();
        startup.start();
    }
    Timer {
        id: startup
        interval: 2000
        onTriggered: {
            root.previous = root.geometry();
            root.ready = true;
        }
    }
    onSignatureChanged: if (ready) {
        const next = geometry();
        for (const name of Object.keys(next))
            if (previous[name] !== next[name] && !pendingNames.includes(name))
                pendingNames = [...pendingNames, name];
        previous = next;
        if (pendingNames.length)
            settle.restart();
    }
    function remember(name) {
        if (rebuilding.includes(name))
            return;
        const v = NacrePanelState.screens[name], p = NacrePanelState.panels[name];
        if (!v || !p)
            return;
        const flags = {};
        for (const key of ["previewOnly", "osd", "session", "launcher", "left", "leftPinned", "dashboard", "dashboardPinned", "dashboardTab", "launcherQuery", "launcherMode", "launcherRoute", "launcherRequest", "edgeMenu"])
            flags[key] = v[key];
        saved = Object.assign({}, saved, {
            [name]: {
                flags: flags,
                ui: Views.capture(p)
            }
        });
    }
    function restoreSaved(data) {
        saved = data.screens || {};
        restoringNames = Object.keys(saved);
        NacrePanelState.settingsPage = data.settingsPage || "appearance";
        restore.start();
    }
    function request(name) {
        if (!Quickshell.screens.some(s => s.name === name))
            return false;
        if (!pendingNames.includes(name))
            pendingNames = [...pendingNames, name];
        settle.restart();
        return true;
    }
    Timer {
        id: settle
        interval: 700
        onTriggered: {
            if (root.rebuilding.length) {
                restart();
                return;
            }
            for (const name of root.pendingNames)
                root.remember(name);
            root.restoringNames = root.pendingNames;
            root.rebuilding = root.pendingNames;
            root.pendingNames = [];
            root.status = NacreI18n.tr("ui.display-recovery.status.refreshing-display-surfaces", "Refreshing display surfaces");
            revive.start();
        }
    }
    Timer {
        id: revive
        interval: 80
        onTriggered: {
            root.rebuilding = [];
            restore.start();
        }
    }
    Timer {
        id: restore
        interval: 180
        onTriggered: {
            let failed = false;
            for (const name of root.restoringNames) {
                if (!Quickshell.screens.some(s => s.name === name))
                    continue;
                const snapshot = root.saved[name], v = NacrePanelState.screens[name], p = NacrePanelState.panels[name];
                if (!v || !p) {
                    failed = true;
                    continue;
                }
                for (const key of Object.keys(snapshot.flags))
                    v[key] = snapshot.flags[key];
                if (v.dashboardTab === 4) {
                    v.dashboardTab = 0;
                    v.dashboard = false;
                    v.dashboardPinned = false;
                }
                Qt.callLater(() => Views.restore(p, snapshot.ui));
            }
            root.status = failed ? NacreI18n.tr("ui.display-recovery.status.using-full-renderer-recovery", "Using full renderer recovery") : NacreI18n.tr("ui.display-recovery.status.display-surfaces-refreshed", "Display surfaces refreshed");
            if (failed)
                fallback.running = true;
            else
                lockLayout.running = true;
        }
    }
    Process {
        id: lockLayout
        command: [NacrePaths.helperBinary, "lock-config"]
    }
    Process {
        id: fallback
        stdinEnabled: true
        command: ["python3", NacrePaths.tools + "/display-recover.py", "-"]
        onStarted: write(JSON.stringify({
            version: 2,
            screens: root.saved,
            settingsPage: NacrePanelState.settingsPage
        }) + "\n")
    }
    IpcHandler {
        target: "displayRecovery"
        function refresh(name: string): bool {
            return root.request(name);
        }
        function state(): string {
            return JSON.stringify({
                status: root.status,
                rebuilding: root.rebuilding,
                outputs: root.geometry()
            });
        }
    }
}
