pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Hyprland
import Quickshell.Services.Notifications
import qs.config

Singleton {
    id: root
    property var list: []
    readonly property var retained: list.filter(entry => entry.retain)
    readonly property var popups: dnd ? [] : list.filter(entry => entry.popup)
    readonly property bool dnd: DesktopSettings.data.dnd === true
    readonly property bool suppressed: Object.values(NacrePanelState.screens).some(view => view.launcher || view.dashboard || view.session) || Object.values(NacrePanelState.panels).some(panel => panel.notifications?.suppressed === true)
    readonly property bool historyVisible: (NacrePanelState.settingsVisible && NacrePanelState.settingsPage === "notifications") || Object.values(NacrePanelState.screens).some(view => view.osd && ["home", "notifications"].includes(view.controlSection || "home")) || Object.values(NacrePanelState.panels).some(panel => panel.popouts?.hasCurrent && panel.popouts.currentName === "notifications")
    onHistoryVisibleChanged: if (historyVisible)
        clock = new Date()
    readonly property bool expire: NacreNotifications.expire
    readonly property int defaultTimeout: NacreNotifications.defaultExpireTimeout
    property date clock: new Date()
    property bool historyReady: false
    property bool dirty: false
    property bool clearedBeforeLoad: false
    property var dismissedKeys: ({})
    property string error: ""
    property var policy: ({
            feedbackApps: [],
            feedbackSummaries: [],
            feedbackPatterns: []
        })
    readonly property string helper: Quickshell.env("HOME") + "/.local/share/nacre/shell/tools/notification-history.py"
    function shouldRetain(app, summary, transient) {
        if (transient)
            return false;
        if (!policy.feedbackApps.includes(app.toLowerCase()))
            return true;
        return !policy.feedbackSummaries.includes(summary) && !policy.feedbackPatterns.some(pattern => new RegExp(pattern).test(summary));
    }
    function receive(notification) {
        const current = list.find(entry => entry.notification === notification || (entry.notification && entry.notification.id === notification.id));
        notification.tracked = true;
        if (current) {
            current.notification = notification;
            current.restartPopup();
            current.popup = !root.dnd;
            if (current.retain)
                persist();
            return current;
        }
        const entry = factory.createObject(root, {
            notification: notification,
            popup: !root.dnd && !notification.lastGeneration
        });
        list = [...list, entry];
        clock = new Date();
        if (entry.retain)
            persist();
        return entry;
    }
    function restore(text) {
        try {
            const rows = JSON.parse(text);
            if (!Array.isArray(rows) || rows.length > 10000 || rows.some(row => !row || typeof row.key !== "string" || !row.key || typeof row.time !== "string" || !Number.isFinite(new Date(row.time).getTime()) || ["summary", "body", "appName", "appIcon", "image"].some(field => row[field] !== undefined && typeof row[field] !== "string")))
                throw new Error();
            const known = new Set(list.map(entry => entry.key));
            const history = [];
            if (!clearedBeforeLoad)
                for (const row of rows) {
                    if (known.has(row.key) || dismissedKeys[row.key] || !shouldRetain(row.appName || "", row.summary || "", false))
                        continue;
                    known.add(row.key);
                    history.push(factory.createObject(root, {
                        key: row.key,
                        time: new Date(row.time),
                        snapshot: row,
                        popup: false
                    }));
                }
            list = [...history, ...list];
            historyReady = true;
            error = "";
            if (dirty)
                saveDelay.restart();
            return true;
        } catch (failure) {
            error = "Notification history could not be read; its file is protected from overwrite.";
            return false;
        }
    }
    function persist() {
        dirty = true;
        if (historyReady)
            saveDelay.restart();
    }
    function remove(entry) {
        if (!list.includes(entry))
            return;
        const wasRetained = entry.retain;
        dismissedKeys = Object.assign({}, dismissedKeys, {
            [entry.key]: true
        });
        list = list.filter(item => item !== entry);
        entry.popup = false;
        entry.hovered = false;
        if (entry.notification)
            entry.notification.dismiss();
        Qt.callLater(() => entry.destroy());
        if (wasRetained)
            persist();
    }
    function dismiss(entry) {
        remove(entry);
    }
    function closed(entry) {
        if (!list.includes(entry))
            return;
        if (!entry.retain)
            remove(entry);
        else
            persist();
    }
    function hideEntry(entry) {
        if (!list.includes(entry))
            return;
        entry.popup = false;
        entry.hovered = false;
        if (!entry.retain)
            remove(entry);
    }
    function expireEntry(entry) {
        if (!list.includes(entry))
            return;
        entry.popup = false;
        entry.hovered = false;
        if (entry.notification)
            entry.notification.expire();
        else
            closed(entry);
    }
    function clearHistory() {
        if (!historyReady)
            clearedBeforeLoad = true;
        for (const entry of [...retained])
            remove(entry);
        persist();
    }
    function hidePopups() {
        for (const entry of [...list])
            if (entry.popup)
                expireEntry(entry);
    }
    function save() {
        if (!historyReady || !dirty || writer.running)
            return;
        dirty = false;
        writer.running = true;
    }
    onSuppressedChanged: if (suppressed)
        for (const entry of list)
            entry.hovered = false
    Component {
        id: factory
        NacreNotificationEntry {
            owner: root
        }
    }
    FileView {
        path: Quickshell.env("HOME") + "/.local/share/nacre/shell/tools/notification-policy.json"
        printErrors: false
        onLoaded: {
            try {
                root.policy = JSON.parse(text());
            } catch (failure) {}
        }
    }
    Timer {
        objectName: "noticeClockTimer"
        interval: 60000
        repeat: true
        running: root.popups.length > 0 || root.historyVisible
        onTriggered: root.clock = new Date()
    }
    Timer {
        id: saveDelay
        objectName: "noticeSaveDelay"
        interval: 100
        onTriggered: root.save()
    }
    Process {
        id: reader
        objectName: "noticeHistoryReader"
        command: ["python3", root.helper, "load"]
        running: true
        stdout: StdioCollector {
            onStreamFinished: root.restore(text)
        }
        onExited: (exitCode, exitStatus) => {
            if (exitCode !== 0)
                root.error = "Notification history could not be read; its file is protected from overwrite.";
        }
    }
    Process {
        id: writer
        objectName: "noticeHistoryWriter"
        stdinEnabled: true
        command: ["python3", root.helper, "save"]
        onStarted: write(JSON.stringify(root.retained.map(entry => entry.record())) + "\n")
        onExited: (exitCode, exitStatus) => {
            if (exitCode !== 0) {
                root.dirty = true;
                root.error = "Notification history could not be saved; it remains in memory.";
            } else if (root.dirty)
                saveDelay.restart();
        }
    }
    NotificationServer {
        keepOnReload: false
        actionsSupported: true
        imageSupported: true
        persistenceSupported: true
        bodyMarkupSupported: false
        onNotification: notification => root.receive(notification)
    }
    GlobalShortcut {
        appid: "nacre_shell"
        name: "clearNotifs"
        onPressed: root.hidePopups()
    }
    IpcHandler {
        target: "notifs"
        function counts(): string {
            return JSON.stringify({
                popups: root.popups.length,
                retained: root.retained.length,
                timers: root.list.filter(entry => entry.popup).map(entry => ({
                            hovered: entry.hovered,
                            timeoutMs: entry.timeout,
                            intervalMs: entry.deadline.interval,
                            running: entry.deadline.running
                        })),
                historyReady: root.historyReady,
                error: root.error
            });
        }
        function clear(): void {
            root.hidePopups();
        }
        function dismissKey(key: string): void {
            const entry = root.list.find(item => item.key === key);
            if (entry)
                root.dismiss(entry);
            else {
                root.dismissedKeys = Object.assign({}, root.dismissedKeys, {
                    [key]: true
                });
                root.persist();
            }
        }
        function clearHistory(): void {
            root.clearHistory();
        }
    }
}
