import QtQuick
import "layout.js" as Layout
import Quickshell
import qs.config
import qs.modules.bar.popouts as Popouts
import qs.modules.dashboard as Dashboard
import qs.modules.extras as Extras
import qs.modules.launcher as Launcher
import qs.modules.notifications as Notifications
import qs.modules.osd as Osd
import qs.modules.session as Session

Item {
    id: root
    required property var screen
    required property var visibilities
    required property var input
    readonly property alias leftDrawer: left
    readonly property alias dashboard: dashboard
    readonly property alias launcher: launcher
    readonly property alias notifications: notifications
    readonly property alias session: session
    readonly property alias osd: osd
    readonly property alias popouts: popouts
    readonly property alias feedback: feedback
    readonly property bool hovered: input.hovered
    readonly property bool popoutHovered: input.popoutHovered
    readonly property bool dashboardHovered: input.dashboardHovered
    anchors.leftMargin: NacreFrame.left
    anchors.rightMargin: NacreFrame.right
    anchors.topMargin: NacreFrame.headerHeight
    anchors.bottomMargin: NacreFrame.bottom
    function position(edge, item, offset) {
        return Layout.attached(edge, width, height, item.width, item.height, offset);
    }
    readonly property var leftPosition: position("left", left)
    readonly property var osdPosition: position("right", osd, session.width)
    readonly property var sessionPosition: position("right", session)
    readonly property var dashboardPosition: position("top", dashboard)
    readonly property var launcherPosition: position("bottom", launcher)
    readonly property var popupPosition: Layout.popout(x, width, popouts.currentCenter, popouts.width, popouts.targetWidth, NacreFrame.rounding)
    Extras.LeftDrawer {
        id: left
        enabled: root.visibilities.left
        screen: root.screen
        visibilities: root.visibilities
        x: root.leftPosition.x
        y: root.leftPosition.y
        clip: true
    }
    Osd.NacreOsdPanel {
        id: osd
        enabled: visibility
        section: root.visibilities.controlSection || "home"
        keyboardActive: root.visibilities.edgeMenu === "osd" && !root.visibilities.previewOnly
        onSectionRequested: section => root.visibilities.controlSection = section
        screen: root.screen
        visibility: root.visibilities.osd && !root.visibilities.session
        x: root.osdPosition.x
        y: root.osdPosition.y
        clip: true
    }
    Session.NacreSessionPanel {
        id: session
        enabled: root.visibilities.session
        visibilities: root.visibilities
        x: root.sessionPosition.x
        y: root.sessionPosition.y
        clip: true
    }
    Dashboard.NacreDashboardPanel {
        id: dashboard
        enabled: root.visibilities.dashboard
        visibilities: root.visibilities
        x: root.dashboardPosition.x
        y: root.dashboardPosition.y
        clip: true
    }
    Popouts.NacrePopupPanel {
        id: popouts
        enabled: hasCurrent
        screen: root.screen
        readonly property bool joinsRight: root.popupPosition.joinsRight
        x: root.popupPosition.x
        y: root.popupPosition.y
        clip: true
    }
    Notifications.NacreNotificationStack {
        id: notifications
        enabled: !suppressed
        suppressed: popouts.height > 0.1 || session.width > 0.1 || dashboard.height > 0.1 || root.visibilities.osd
        anchors.right: parent.right
        anchors.top: parent.top
    }
    Launcher.NacreLauncherPanel {
        id: launcher
        enabled: root.visibilities.launcher
        visibilities: root.visibilities
        x: root.launcherPosition.x
        y: root.launcherPosition.y
        clip: true
        z: 10
    }
    Osd.NacreOsdEvents {
        id: feedback
        screen: root.screen
        visibilities: root.visibilities
        hovered: false
    }
}
