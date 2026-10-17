import QtQuick

// One latest query in flight. A panel presents without waiting for this object.
QtObject {
    id: root
    property bool enabled: NacreDaemon.enabled
    property bool active: true
    property bool presented: active
    property string query: ""
    property string route: "home"
    property string resultFilter: "all"
    property string category: "all"
    property var result: ({})
    property string error: ""
    property string resultKey: ""
    property string resultBoot: ""
    property bool pending: false
    property bool dirty: false
    property bool subscribed: false
    readonly property string key: JSON.stringify({
        query: query,
        route: route,
        filter: resultFilter,
        category: category
    })
    readonly property bool current: enabled && active && NacreDaemon.connected && resultKey === key && resultBoot === NacreDaemon.boot
    readonly property var menuState: NacreDaemon.topic("menu")
    readonly property var entries: (result.order || []).map(id => {
        const item = result.items[id];
        const view = item.presentation || {};
        const entry = Object.assign({}, view, item, {
            id: view.id || id,
            icon: item.icon || "",
            daemonId: id,
            daemonTicket: item.activation,
            daemonRevision: result.catalog_rev,
            daemonQueryKey: resultKey,
            daemonRowId: view.id || id,
            daemonOperation: view.operation
        });
        delete entry.presentation;
        delete entry.activation;
        return entry;
    })
    function schedule() {
        if (!enabled || !active)
            return;
        dirty = true;
        coalesce.restart();
    }
    function configure() {
        if (enabled && active && !subscribed) {
            subscribed = true;
            NacreDaemon.subscribe("menu");
        } else if ((!enabled || !active) && subscribed) {
            subscribed = false;
            NacreDaemon.unsubscribe("menu");
        }
        if (!enabled || !active) {
            coalesce.stop();
            dirty = false;
            if (!enabled || !presented)
                releaseResult();
            return;
        }
        schedule();
    }
    function releaseResult() {
        if (Object.keys(result).length)
            result = ({});
        resultKey = "";
        resultBoot = "";
        error = "";
    }
    function run() {
        if (!dirty || pending || !enabled || !active || !NacreDaemon.connected)
            return;
        dirty = false;
        const wanted = key;
        const boot = NacreDaemon.boot;
        const params = {
            query: query,
            route: route,
            mode: "apps",
            filter: resultFilter,
            category: category,
            presentation: true
        };
        pending = true;
        NacreDaemon.call("menu.query", params, (value, failure) => {
            root.pending = false;
            if (root.enabled && root.active && root.key === wanted && NacreDaemon.boot === boot) {
                if (!failure && (!value || !Array.isArray(value.order) || !value.items || value.order.length > 2048 || value.order.some(id => typeof id !== "string" || !Object.prototype.hasOwnProperty.call(value.items, id))))
                    failure = {
                        message: NacreI18n.tr("ui.menu-query.error.invalid-response", "Invalid launcher response")
                    };
                root.error = failure?.message || "";
                if (!failure) {
                    root.result = value;
                    root.resultKey = wanted;
                    root.resultBoot = boot;
                }
            }
            if (root.dirty)
                coalesce.restart();
        }, 45000);
    }
    function activate(entry, callback) {
        if (!current || entry.daemonQueryKey !== resultKey || !entry.daemonTicket) {
            callback(null, {
                code: "stale_revision",
                message: NacreI18n.tr("ui.menu-query.error.catalog-changed", "Launcher changed; query again"),
                retry: true,
                dispatched: false
            });
            schedule();
            return;
        }
        NacreDaemon.call("menu.activate", {
            id: entry.daemonId,
            activation: entry.daemonTicket,
            catalog_rev: entry.daemonRevision
        }, callback, 45000);
    }
    onKeyChanged: schedule()
    onEnabledChanged: configure()
    onActiveChanged: configure()
    onPresentedChanged: if (!presented && !active)
        releaseResult()
    Component.onCompleted: configure()
    Component.onDestruction: if (subscribed)
        NacreDaemon.unsubscribe("menu")
    readonly property Timer coalesce: Timer {
        interval: 0
        onTriggered: root.run()
    }
    readonly property Connections catalogChanges: Connections {
        target: root.menuState
        function onDataChanged() {
            root.schedule();
        }
        function onFreshChanged() {
            if (root.menuState.fresh)
                root.schedule();
        }
    }
    readonly property Connections connectionChanges: Connections {
        target: NacreDaemon
        function onConnectedChanged() {
            if (NacreDaemon.connected)
                root.schedule();
        }
    }
}
