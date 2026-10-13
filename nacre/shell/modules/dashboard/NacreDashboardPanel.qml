pragma ComponentBehavior: Bound
import QtQuick
import Quickshell
import qs.widgets
import qs.services

NacreSurface {
    id: root
    required property PersistentProperties visibilities
    property string screenName: ""
    function prepareLift() {
        edgeOpening.complete();
    }
    readonly property bool opening: visibilities.dashboard && edgeOpening.running
    property bool restoreImmediately: false
    function restoreAttached() {
        edgeOpening.complete();
        presentedHeight = Qt.binding(() => visibilities.dashboard ? contentHeight : 0);
    }
    property bool handoffHidden: false
    opacity: handoffHidden ? 0 : 1
    onEnabledChanged: if (enabled)
        handoffHidden = false
    property bool detached: false
    property bool hostActive: true
    readonly property real viewportWidth: parent?.width ?? 1200
    readonly property real viewportHeight: parent?.height ?? 1000
    readonly property var names: NacreDashboardRegistry.names
    readonly property int currentIndex: names[visibilities.dashboardTab] && NacreDashboardState.tabs.includes(names[visibilities.dashboardTab]) ? visibilities.dashboardTab : Math.max(0, names.indexOf(NacreDashboardState.tabs[0]))
    readonly property bool updating: hostActive && visibilities.dashboard && visible && (detached || NacrePanelState.hidden !== true)
    readonly property real contentWidth: Math.max(0, Math.min(detached ? viewportWidth : viewportWidth - 48, 984))
    readonly property real contentHeight: Math.max(0, detached ? viewportHeight : Math.min(viewportHeight - 64, 650))
    property bool customizing: false
    property real presentedHeight: visibilities.dashboard ? contentHeight : 0
    property int slideDirection: 1
    implicitWidth: contentWidth
    implicitHeight: Math.max(0, presentedHeight)
    visible: height > 0
    color: NacreTokens.body
    clip: true
    radius: 18
    function syncIndex() {
        if (visibilities.dashboardTab !== currentIndex)
            visibilities.dashboardTab = currentIndex;
    }
    function focusPage() {
        if (!root.updating || root.customizing)
            return;
        (root.currentIndex >= 4 ? extra : page).forceActiveFocus();
    }
    onCurrentIndexChanged: {
        Qt.callLater(syncIndex);
        Qt.callLater(focusPage);
    }
    function select(index) {
        if (!names[index] || !NacreDashboardState.tabs.includes(names[index]))
            return;
        slideDirection = index >= currentIndex ? 1 : -1;
        visibilities.dashboardTab = index;
        visibilities.dashboardPinned = false;
        customizing = false;
        if (NacreTokens.motionEnabled)
            pageMotion.restart();
    }
    NacreDashboardNavigation {
        id: navigation
        objectName: "dashboardNavigation"
        x: 24
        y: 12
        width: Math.max(0, root.width - 142)
        currentIndex: root.currentIndex
        onSelected: index => root.select(index)
    }
    ActionButton {
        x: root.width - 100
        y: 16
        text: ""
        icon: "tune"
        compact: true
        accessibleLabel: NacreI18n.tr("dashboard.dashboard-panel.accessibleLabel.customize-dashboard", "Customize dashboard")
        onClicked: root.customizing = !root.customizing
    }
    ActionButton {
        x: root.width - 58
        y: 16
        text: ""
        icon: "settings"
        compact: true
        accessibleLabel: NacreI18n.tr("dashboard.dashboard-panel.accessibleLabel.open-settings", "Open Settings")
        onClicked: NacrePanelState.openSettings("")
    }
    Item {
        id: stage
        x: 22
        y: 68
        width: Math.max(0, root.width - 44)
        height: Math.max(0, root.contentHeight - y - 20)
        clip: true
        Loader {
            id: page
            objectName: "dashboardPage"
            asynchronous: true
            width: parent.width
            height: parent.height
            focus: true
            onLoaded: Qt.callLater(root.focusPage)
            sourceComponent: root.customizing ? customization : root.currentIndex < 4 ? [today, weather, media, system][root.currentIndex] : null
            active: root.currentIndex < 4 || root.customizing ? root.hostActive && (root.detached || root.visibilities.dashboard || root.presentedHeight > 0) && (root.detached || NacrePanelState.hidden !== true) : false
        }
    }
    Loader {
        id: extra
        objectName: "dashboardExtraPage"
        x: stage.x
        y: stage.y
        width: stage.width
        height: stage.height
        asynchronous: true
        active: root.updating && !root.customizing && root.currentIndex >= 4
        focus: active
        function loadPage() {
            const descriptor = NacreDashboardRegistry.entry(root.names[root.currentIndex]);
            if (!active || !descriptor) {
                source = "";
                return;
            }
            setSource(Qt.resolvedUrl("extras/" + descriptor.page), {
                shouldUpdate: root.updating
            });
        }
        onActiveChanged: loadPage()
        Connections {
            target: root
            function onCurrentIndexChanged() {
                extra.loadPage();
            }
        }
        onLoaded: {
            item.shouldUpdate = Qt.binding(() => root.updating);
            Qt.callLater(root.focusPage);
        }
    }
    ParallelAnimation {
        id: pageMotion
        NumberAnimation {
            target: page
            property: "x"
            from: root.slideDirection * 24
            to: 0
            duration: 180
            easing.type: Easing.OutCubic
        }
        NumberAnimation {
            target: page
            property: "opacity"
            from: .55
            to: 1
            duration: 180
        }
    }
    Connections {
        target: NacreTokens
        function onMotionEnabledChanged() {
            if (!NacreTokens.motionEnabled) {
                pageMotion.complete();
            }
        }
    }
    Component {
        id: today
        NacreTodayPage {
            shouldUpdate: root.updating
            visibilities: root.visibilities
        }
    }
    Component {
        id: weather
        NacreWeatherPage {
            shouldUpdate: root.updating
        }
    }
    Component {
        id: media
        NacreMediaPage {
            shouldUpdate: root.updating
            visibilities: root.visibilities
        }
    }
    Component {
        id: system
        NacreSystemPage {
            shouldUpdate: root.updating
        }
    }
    Component {
        id: customization
        NacreDashboardCustomize {}
    }
    Behavior on presentedHeight {
        enabled: !root.restoreImmediately || !root.visibilities.dashboard
        NumberAnimation {
            id: edgeOpening
            duration: NacreTokens.motionEnabled ? 360 : 0
            easing.type: Easing.InOutCubic
        }
    }
    Keys.onPressed: event => {
        if (!root.customizing && (event.modifiers & Qt.ControlModifier) && event.key === Qt.Key_Tab) {
            const order = NacreDashboardState.tabs, before = order.indexOf(root.names[root.currentIndex]), delta = event.modifiers & Qt.ShiftModifier ? -1 : 1;
            root.select(root.names.indexOf(order[(before + delta + order.length) % order.length]));
            event.accepted = true;
        } else if ((event.key >= Qt.Key_1 && event.key <= Qt.Key_9 || event.key === Qt.Key_0) && !event.modifiers && !root.customizing) {
            const name = NacreDashboardState.tabs[event.key === Qt.Key_0 ? 9 : event.key - Qt.Key_1];
            if (name)
                root.select(root.names.indexOf(name));
            event.accepted = true;
        }
    }
    Keys.onEscapePressed: {
        if (customizing)
            customizing = false;
        else {
            if (!root.detached)
                visibilities.dashboard = false;
            visibilities.dashboardPinned = false;
        }
    }
}
