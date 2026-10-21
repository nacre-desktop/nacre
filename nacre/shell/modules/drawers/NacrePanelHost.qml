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
        return drag?.host === item ? Qt.point(Math.round(drag.body.x + drag.dx) - screen.x - x, Math.round(drag.body.y + drag.dy) - screen.y - y) : position;
    }
    function sensorPosition(item, position) {
        const drag = NacrePearls.drag;
        const sensor = drag?.sensorBody || drag?.body;
        return drag?.host === item ? Qt.point(sensor.x - screen.x - x, sensor.y - screen.y - y) : position;
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
                color: NacreTokens.gradientColour(Qt.alpha(NacreTokens.accent, .65), Qt.alpha(NacreTokens.orientSecondary, .65))
            }
            GradientStop {
                position: 1
                color: NacreTokens.gradientColour(Qt.alpha(NacreTokens.accent, .65), Qt.alpha(NacreTokens.accent, .65))
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
    readonly property var osdPosition: Qt.point(width - osd.width - session.width, 0)
    readonly property var sessionPosition: position("right", session)
    readonly property var dashboardPosition: position("top", dashboard)
    readonly property var launcherPosition: position("bottom", launcher)
    readonly property var popupPosition: Layout.popout(x, width, popouts.currentCenter, popouts.width, popouts.targetWidth, NacreFrame.rounding)
    Extras.LeftDrawer {
        id: left
        restoreImmediately: root.visibilities.hoverDocked === true
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
            // The sensor stays on the screen while its live panel moves.
            parent: root
            x: root.sensorPosition(left, root.leftPosition).x
            y: root.sensorPosition(left, root.leftPosition).y
            width: left.width
            height: left.height
            z: 20
            kind: "ai"
            host: left
            screen: root.screen
        }
        clip: true
    }
    Osd.NacreOsdPanel {
        id: osd
        restoreImmediately: root.visibilities.hoverDocked === true
        enabled: visibility
        section: root.visibilities.controlSection || "home"
        presentation: root.visibilities.controlsMode || "sidebar"
        onSidebarRequested: pointerOwned => {
            root.visibilities.controlsMode = "sidebar";
            root.visibilities.controlSection = "home";
            NacrePanelState.pinControls(root.screen.name, pointerOwned);
        }
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
            // The sensor stays on the screen while its live panel moves.
            parent: root
            x: root.sensorPosition(osd, root.osdPosition).x
            y: root.sensorPosition(osd, root.osdPosition).y
            width: osd.width
            height: osd.height
            z: 20
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
        restoreImmediately: root.visibilities.hoverDocked === true
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
            // The sensor stays on the screen while its live panel moves.
            parent: root
            x: root.sensorPosition(dashboard, root.dashboardPosition).x
            y: root.sensorPosition(dashboard, root.dashboardPosition).y
            width: dashboard.width
            height: dashboard.height
            z: 20
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
        suppressed: popouts.height > 0.1 || session.width > 0.1 || root.visibilities.osd
        anchors.right: parent.right
        anchors.top: parent.top
    }
    Launcher.NacreLauncherPanel {
        id: launcher
        restoreImmediately: root.visibilities.hoverDocked === true
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
            // The sensor stays on the screen while its live panel moves.
            parent: root
            x: root.sensorPosition(launcher, root.launcherPosition).x
            y: root.sensorPosition(launcher, root.launcherPosition).y
            width: launcher.width
            height: launcher.height
            z: 20
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
