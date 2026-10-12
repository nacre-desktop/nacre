import QtQuick
import "layout.js" as Layout
import Quickshell
import qs.services
import qs.widgets
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
    function moved(item, position) {
        const drag = NacrePearls.drag;
        return drag?.host === item ? Qt.point(position.x + drag.dx, position.y + drag.dy) : position;
    }
    // Keep the actual popup alive during the native-window handoff.
    readonly property var movingPanel: NacrePearls.drag?.screen === screen.name ? NacrePearls.drag.host : null
    Rectangle {
        visible: !!root.movingPanel
        x: (root.movingPanel?.x ?? 0) - 1
        y: (root.movingPanel?.y ?? 0) - 1
        width: (root.movingPanel?.width ?? 0) + 2
        height: (root.movingPanel?.height ?? 0) + 2
        radius: 18
        gradient: Gradient {
            orientation: Gradient.Horizontal
            GradientStop {
                position: 0
                color: Qt.alpha(NacreTokens.accent, .65)
            }
            GradientStop {
                position: .5
                color: Qt.alpha(NacreTokens.orientSecondary, .65)
            }
            GradientStop {
                position: 1
                color: Qt.alpha(NacreTokens.accent, .65)
            }
        }
        NacreSurface {
            anchors.fill: parent
            anchors.margins: 1
            radius: 17
            color: NacreTokens.body
        }
        z: 0
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
        Behavior on x {
            NumberAnimation {
                duration: NacreTokens.motionEnabled && NacrePearls.drag?.released ? 180 : 0
                easing.type: Easing.OutCubic
            }
        }
        Behavior on y {
            NumberAnimation {
                duration: NacreTokens.motionEnabled && NacrePearls.drag?.released ? 180 : 0
                easing.type: Easing.OutCubic
            }
        }
        x: root.moved(left, root.leftPosition).x
        y: root.moved(left, root.leftPosition).y
        NacrePanelDrag {
            kind: "ai"
            host: left
            screen: root.screen
        }
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
        Behavior on x {
            NumberAnimation {
                duration: NacreTokens.motionEnabled && NacrePearls.drag?.released ? 180 : 0
                easing.type: Easing.OutCubic
            }
        }
        Behavior on y {
            NumberAnimation {
                duration: NacreTokens.motionEnabled && NacrePearls.drag?.released ? 180 : 0
                easing.type: Easing.OutCubic
            }
        }
        x: root.moved(osd, root.osdPosition).x
        y: root.moved(osd, root.osdPosition).y
        NacrePanelDrag {
            kind: "controls"
            host: osd
            screen: root.screen
        }
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
        screenName: root.screen.name
        enabled: root.visibilities.dashboard
        visibilities: root.visibilities
        Behavior on x {
            NumberAnimation {
                duration: NacreTokens.motionEnabled && NacrePearls.drag?.released ? 180 : 0
                easing.type: Easing.OutCubic
            }
        }
        Behavior on y {
            NumberAnimation {
                duration: NacreTokens.motionEnabled && NacrePearls.drag?.released ? 180 : 0
                easing.type: Easing.OutCubic
            }
        }
        x: root.moved(dashboard, root.dashboardPosition).x
        y: root.moved(dashboard, root.dashboardPosition).y
        NacrePanelDrag {
            kind: "dashboard"
            host: dashboard
            screen: root.screen
        }
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
        screenName: root.screen.name
        enabled: root.visibilities.launcher
        visibilities: root.visibilities
        Behavior on x {
            NumberAnimation {
                duration: NacreTokens.motionEnabled && NacrePearls.drag?.released ? 180 : 0
                easing.type: Easing.OutCubic
            }
        }
        Behavior on y {
            NumberAnimation {
                duration: NacreTokens.motionEnabled && NacrePearls.drag?.released ? 180 : 0
                easing.type: Easing.OutCubic
            }
        }
        x: root.moved(launcher, root.launcherPosition).x
        y: root.moved(launcher, root.launcherPosition).y
        NacrePanelDrag {
            kind: "launcher"
            host: launcher
            screen: root.screen
        }
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
