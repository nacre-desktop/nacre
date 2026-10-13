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
    property bool handoffHidden: false
    opacity: handoffHidden ? 0 : 1
    onEnabledChanged: if (enabled)
        handoffHidden = false
    property bool detached: false
    property bool hostActive: true
    readonly property real viewportWidth: parent?.width ?? 1200
    readonly property real viewportHeight: parent?.height ?? 1000
    readonly property var names: ["today", "weather", "media", "system", "focus"]
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
    onCurrentIndexChanged: Qt.callLater(syncIndex)
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
            active: root.detached || root.visibilities.dashboard || root.presentedHeight > 0
            width: parent.width
            height: parent.height
            focus: true
            sourceComponent: root.customizing ? customization : [today, weather, media, system, focusPage][root.currentIndex]
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
        id: focusPage
        NacreFocusPage {
            shouldUpdate: root.updating
        }
    }
    Component {
        id: customization
        NacreDashboardCustomize {}
    }
    Behavior on presentedHeight {
        NumberAnimation {
            id: edgeOpening
            duration: NacreTokens.motionEnabled ? 360 : 0
            easing.type: Easing.InOutCubic
        }
    }
    Keys.onPressed: event => {
        if (event.key >= Qt.Key_1 && event.key <= Qt.Key_5 && !event.modifiers && !root.customizing) {
            const name = NacreDashboardState.tabs[event.key - Qt.Key_1];
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
