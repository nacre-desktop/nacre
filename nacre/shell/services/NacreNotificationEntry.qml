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
        return seconds < 60 ? NacreI18n.tr("notifications.time.now", "now") : seconds < 3600 ? NacreI18n.tr("notifications.time.minutes", {
            one: "{count}m",
            other: "{count}m"
        }, {
            count: Math.floor(seconds / 60)
        }) : seconds < 86400 ? NacreI18n.tr("notifications.time.hours", {
            one: "{count}h",
            other: "{count}h"
        }, {
            count: Math.floor(seconds / 3600)
        }) : NacreI18n.tr("notifications.time.days", {
            one: "{count}d",
            other: "{count}d"
        }, {
            count: Math.floor(seconds / 86400)
        });
    }
    readonly property real timeout: notification?.expireTimeout ?? -1
    readonly property Timer deadline: Timer {
        interval: root.timeout > 0 ? Math.max(1, Math.min(root.timeout, 2147483647)) : root.owner.defaultTimeout
        // History/DND visibility must not keep native notifications tracked.
        running: !!root.notification && root.timeout !== 0 && !root.hovered && root.owner.expire
        onTriggered: root.owner.expireEntry(root)
    }
    readonly property Timer popupDeadline: Timer {
        interval: root.timeout > 0 ? 10000 : root.owner.defaultTimeout
        running: root.popup && !!root.notification && (root.timeout === 0 || root.timeout > 10000) && !root.hovered && !root.owner.dnd && root.owner.expire
        onTriggered: root.owner.hideEntry(root)
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
        // Native image-provider URLs belong to the live notification. History
        // keeps its text, but must not keep an expired image or stale URL alive.
        saved.image = "";
        notification = null;
        snapshot = saved;
    }
    function release() {
        const live = notification;
        popup = false;
        hovered = false;
        notification = null;
        snapshot = ({});
        if (live)
            live.dismiss();
    }
    function restartPopup() {
        time = new Date();
        popup = false;
        popup = true;
        hovered = false;
    }
}
