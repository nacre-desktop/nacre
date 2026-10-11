import QtQuick
import qs.config
import qs.services

Item {
    id: root
    required property var screen
    required property var panels
    required property var visibilities
    property var lips: null
    property bool leftWasHovered: false
    property bool ready: false
    Component.onCompleted: {
        ready = true;
        settleHover();
    }
    property bool hidden: NacrePanelState.hidden
    readonly property real padding: Math.max(12, NacreFrame.rounding / 2)
    readonly property bool modal: !hidden && !visibilities.previewOnly && (visibilities.launcher || visibilities.session || visibilities.dashboard && visibilities.dashboardPinned || panels.popouts.pinned && panels.popouts.hasCurrent || visibilities.edgeMenu === "dashboard" && visibilities.dashboard || visibilities.edgeMenu === "left" && visibilities.left && !visibilities.leftPinned || visibilities.edgeMenu === "osd" && visibilities.osd)
    readonly property bool autoEdges: !hidden && !modal && DesktopSettings.data.clickEdgeMenus !== true && !visibilities.launcher && !visibilities.session && !NacreHoverIntent.fullscreenFor(screen.name)
    readonly property bool lipsAvailable: !hidden && !modal && !visibilities.launcher && !visibilities.session && !visibilities.dashboardPinned && !NacreHoverIntent.fullscreenFor(screen.name)
    readonly property bool leftEdgeAvailable: lipsAvailable && DesktopSettings.data.leftDrawer !== false && !visibilities.left
    readonly property bool rightEdgeAvailable: lipsAvailable && DesktopSettings.data.rightEdge !== false && !visibilities.osd
    readonly property bool topEdgeAvailable: lipsAvailable && !visibilities.dashboard
    readonly property rect leftEdgeRect: lips?.leftRect ?? Qt.rect(0, (height - 144) / 2, NacreFrame.left + 5, 144)
    readonly property rect rightEdgeRect: lips?.rightRect ?? Qt.rect(width - NacreFrame.right - 5, (height - 144) / 2, NacreFrame.right + 5, 144)
    readonly property rect topEdgeRect: lips?.topRect ?? Qt.rect((width - 208) / 2, NacreFrame.headerHeight - 1, 208, 7)
    readonly property rect headerRect: !hidden && DesktopSettings.data.topEdge !== false ? Qt.rect(0, 0, width, NacreFrame.headerHeight) : Qt.rect(0, 0, 0, 0)
    readonly property rect launcherRect: box(panels.launcher, visibilities.launcher)
    readonly property rect dashboardRect: box(panels.dashboard, visibilities.dashboard)
    readonly property rect leftRect: box(panels.leftDrawer, visibilities.left)
    readonly property rect osdRect: box(panels.osd, visibilities.osd && !visibilities.session)
    readonly property rect sessionRect: box(panels.session, visibilities.session)
    readonly property rect popoutRect: box(panels.popouts, panels.popouts.hasCurrent)
    readonly property rect notificationRect: box(panels.notifications, panels.notifications.visible)
    readonly property var regions: hidden ? [] : modal ? [Qt.rect(0, 0, width, height)] : [headerRect, launcherRect, dashboardRect, leftRect, osdRect, sessionRect, popoutRect, notificationRect, leftEdgeAvailable ? leftEdgeRect : Qt.rect(0, 0, 0, 0), rightEdgeAvailable ? rightEdgeRect : Qt.rect(0, 0, 0, 0), topEdgeAvailable ? topEdgeRect : Qt.rect(0, 0, 0, 0)].filter(r => r.width > 0 && r.height > 0)
    readonly property bool hovered: hover.hovered
    readonly property point pointer: hover.point.position
    readonly property bool dashboardHovered: hovered && inside(expand(dashboardRect), pointer)
    readonly property bool leftHovered: hovered && inside(expand(leftRect), pointer)
    readonly property bool osdHovered: hovered && inside(expand(osdRect), pointer)
    readonly property bool popoutHovered: hovered && inside(expand(popoutRect), pointer)
    function box(item, active) {
        if (hidden || !active || !item || item.width <= 0 || item.height <= 0)
            return Qt.rect(0, 0, 0, 0);
        return Qt.rect(panels.x + item.x, panels.y + item.y, item.width, item.height);
    }
    function expand(rect) {
        return rect.width > 0 && rect.height > 0 ? Qt.rect(rect.x - padding, rect.y - padding, rect.width + padding * 2, rect.height + padding * 2) : rect;
    }
    function inside(rect, point) {
        return rect.width > 0 && rect.height > 0 && point.x >= rect.x && point.x < rect.x + rect.width && point.y >= rect.y && point.y < rect.y + rect.height;
    }
    function containsPanel(point) {
        return [launcherRect, dashboardRect, leftRect, osdRect, sessionRect, popoutRect, notificationRect].some(rect => inside(rect, point));
    }
    function dismiss() {
        NacreHoverIntent.dismiss(screen, hovered);
        visibilities.launcher = false;
        visibilities.session = false;
        visibilities.dashboard = false;
        visibilities.dashboardPinned = false;
        visibilities.osd = false;
        if (!visibilities.leftPinned)
            visibilities.left = false;
        panels.popouts.hasCurrent = false;
        panels.popouts.pinned = false;
        visibilities.edgeMenu = "";
    }
    function outsideClick(point) {
        if (modal && !containsPanel(point))
            dismiss();
    }
    function settleHover() {
        if (!ready || !panels || !visibilities)
            return;
        if (hidden || modal) {
            leftExit.stop();
            dashboardExit.stop();
            popoutExit.stop();
            osdExit.stop();
            return;
        }
        if (visibilities.left && leftWasHovered && !visibilities.leftPinned && !leftHovered) {
            if (!leftExit.running)
                leftExit.start();
        } else
            leftExit.stop();
        if (visibilities.dashboard && !visibilities.dashboardPinned && !dashboardHovered && !NacreHoverIntent.headers[screen.name]) {
            if (!dashboardExit.running)
                dashboardExit.start();
        } else
            dashboardExit.stop();
        if (panels.popouts.hasCurrent && !panels.popouts.pinned && !popoutHovered && !panels.popouts.headerHovered) {
            if (!popoutExit.running)
                popoutExit.start();
        } else
            popoutExit.stop();
    }
    onHoveredChanged: settleHover()
    onPointerChanged: {
        if (ready) {
            NacreHoverIntent.observe(screen, pointer.x, pointer.y);
            settleHover();
        }
    }
    onModalChanged: settleHover()
    onLeftRectChanged: settleHover()
    onOsdRectChanged: settleHover()
    onDashboardRectChanged: settleHover()
    onLeftHoveredChanged: {
        if (leftHovered)
            leftWasHovered = true;
        settleHover();
    }
    onDashboardHoveredChanged: settleHover()
    onPopoutHoveredChanged: settleHover()
    onOsdHoveredChanged: {
        if (osdHovered)
            osdExit.stop();
        else if (!modal && visibilities.osd)
            osdExit.restart();
    }
    Connections {
        target: root.visibilities
        function onLeftChanged() {
            if (!root.visibilities.left) {
                root.leftWasHovered = false;
                leftExit.stop();
            }
        }
    }
    HoverHandler {
        id: hover
        parent: root.parent
        enabled: !root.hidden
        blocking: false
    }
    MouseArea {
        anchors.fill: parent
        enabled: root.modal
        acceptedButtons: Qt.AllButtons
        onPressed: event => root.outsideClick(Qt.point(event.x, event.y))
        onDoubleClicked: event => root.outsideClick(Qt.point(event.x, event.y))
    }
    Timer {
        id: leftExit
        interval: 130
        onTriggered: if (!root.leftHovered && !root.visibilities.leftPinned && root.visibilities.edgeMenu !== "left")
            root.visibilities.left = false
    }
    Timer {
        id: dashboardExit
        interval: 140
        onTriggered: if (!root.dashboardHovered && !root.visibilities.dashboardPinned && !NacreHoverIntent.headers[root.screen.name] && root.visibilities.edgeMenu !== "dashboard")
            root.visibilities.dashboard = false
    }
    Timer {
        id: popoutExit
        interval: 120
        onTriggered: if (!root.popoutHovered && !root.panels.popouts.headerHovered && !root.panels.popouts.pinned)
            root.panels.popouts.hasCurrent = false
    }
    Timer {
        id: osdExit
        interval: 130
        onTriggered: if (!root.osdHovered && root.visibilities.edgeMenu !== "osd")
            root.visibilities.osd = false
    }
    function describe() {
        return {
            modal: modal,
            hidden: hidden,
            inputRegions: regions.map(r => ({
                        x: r.x,
                        y: r.y,
                        width: r.width,
                        height: r.height
                    })),
            dashboard: dashboardRect,
            left: leftRect,
            osd: osdRect,
            launcher: launcherRect,
            lips: {
                top: topEdgeRect,
                left: leftEdgeRect,
                right: rightEdgeRect
            },
            hovered: hovered
        };
    }
}
