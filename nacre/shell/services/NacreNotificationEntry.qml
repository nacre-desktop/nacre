import QtQuick

QtObject {
    id: root
    required property var owner
    property var notification: null
    property var snapshot: ({})
    property string key: Date.now().toString(36) + "-" + Math.random().toString(36).slice(2)
    property date time: new Date()
    property bool hovered: false
    property bool popup: false
    readonly property string summary: notification?.summary ?? snapshot.summary ?? ""
    readonly property string body: notification?.body ?? snapshot.body ?? ""
    readonly property string appName: notification?.appName ?? snapshot.appName ?? ""
    readonly property string appIcon: notification?.appIcon ?? snapshot.appIcon ?? ""
    readonly property string image: notification?.image ?? snapshot.image ?? ""
    readonly property int urgency: notification?.urgency ?? snapshot.urgency ?? 1
    readonly property var actions: notification?.actions ?? []
    readonly property bool retain: owner.shouldRetain(appName, summary, notification?.transient ?? snapshot.transient ?? false)
    readonly property string timeStr: {
        const seconds = Math.max(0, (owner.clock.getTime() - time.getTime()) / 1000);
        return seconds < 60 ? "now" : seconds < 3600 ? Math.floor(seconds / 60) + "m" : seconds < 86400 ? Math.floor(seconds / 3600) + "h" : Math.floor(seconds / 86400) + "d";
    }
    readonly property real timeout: notification?.expireTimeout ?? -1
    readonly property Timer deadline: Timer {
        interval: root.timeout > 0 ? Math.max(1, Math.min(root.timeout, 10000)) : root.owner.defaultTimeout
        running: root.popup && !!root.notification && !root.hovered && !root.owner.dnd && root.owner.expire
        onTriggered: root.timeout === 0 || root.timeout > 10000 ? root.owner.hideEntry(root) : root.owner.expireEntry(root)
    }
    readonly property Connections changes: Connections {
        target: root.notification
        ignoreUnknownSignals: true
        function onClosed(reason) {
            root.freeze();
            root.popup = false;
            root.hovered = false;
            root.owner.closed(root);
        }
        function onSummaryChanged() {
            if (root.retain)
                root.owner.persist();
        }
        function onBodyChanged() {
            if (root.retain)
                root.owner.persist();
        }
        function onAppIconChanged() {
            if (root.retain)
                root.owner.persist();
        }
        function onImageChanged() {
            if (root.retain)
                root.owner.persist();
        }
    }
    function record() {
        return {
            key: key,
            time: time.toISOString(),
            summary: summary,
            body: body,
            appName: appName,
            appIcon: appIcon,
            image: image,
            urgency: urgency,
            transient: notification?.transient ?? snapshot.transient ?? false
        };
    }
    function freeze() {
        const saved = record();
        notification = null;
        snapshot = saved;
    }
    function restartPopup() {
        time = new Date();
        popup = false;
        popup = true;
        hovered = false;
    }
}
