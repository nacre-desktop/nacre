import QtQuick
import qs.widgets
import qs.services
import qs.config

Item {
    id: root
    required property var screen
    required property var visibilities
    required property var controller
    property real focusProgress: 0
    readonly property var rimHandles: []
    readonly property bool available: !NacrePanelState.hidden && !controller.modal && !visibilities.launcher && !visibilities.session && !visibilities.dashboardPinned && !NacreHoverIntent.fullscreenFor(screen.name)
    readonly property rect topRect: NacreFrame.headerHeight > 0 ? Qt.rect((width - 208) / 2, 0, 208, NacreFrame.headerHeight) : Qt.rect(0, 0, 0, 0)
    readonly property rect leftRect: NacreFrame.left > 0 ? Qt.rect(0, (height - 144) / 2, NacreFrame.left, 144) : Qt.rect(0, 0, 0, 0)
    readonly property rect rightRect: NacreFrame.right > 0 ? Qt.rect(width - NacreFrame.right, (height - 144) / 2, NacreFrame.right, 144) : Qt.rect(0, 0, 0, 0)
    function publishRegions() {
        if (!screen?.name)
            return;
        NacreHoverIntent.lipRegions[screen.name] = {
            dashboard: Qt.rect((width - 208) / 2, 0, 208, NacreFrame.headerHeight),
            left: leftRect,
            osd: rightRect,
            resting: {
                left: Qt.rect(0, (height - 144) / 2, NacreFrame.left, 144),
                osd: Qt.rect(width - NacreFrame.right, (height - 144) / 2, NacreFrame.right, 144)
            }
        };
    }
    Component.onCompleted: publishRegions()
    onTopRectChanged: publishRegions()
    onLeftRectChanged: publishRegions()
    onRightRectChanged: publishRegions()
    function approach(name, lip, buttons) {
        const point = lip.mapToItem(root, lip.pointerX, lip.pointerY);
        NacreHoverIntent.observe(screen, point.x, point.y);
        if (name === "dashboard")
            NacreHoverIntent.setHeaderHover(screen.name, "lip", true);
        if (DesktopSettings.data.clickEdgeMenus === true || !available || !NacreHoverIntent.canOpen(name, screen, buttons))
            return;
        if (!visibilities[name])
            NacrePanelState.hoverEdge(name, screen.name);
    }
    function leave(name) {
        if (name === "dashboard")
            NacreHoverIntent.setHeaderHover(screen.name, "lip", false);
        NacreHoverIntent.rearm(name, screen);
        controller.settleHover();
    }
    NacreFrameLip {
        id: top
        decorationEnabled: false
        progress: root.focusProgress
        objectName: "frameTopLip"
        active: root.visibilities.dashboard
        edge: "top"
        x: root.topRect.x
        y: root.topRect.y
        width: root.topRect.width
        height: root.topRect.height
        visible: NacrePearls.drag?.kind !== "dashboard" && !NacrePanelState.hidden && width > 0
        onEntered: buttons => root.approach("dashboard", this, buttons)
        onMoved: buttons => root.approach("dashboard", this, buttons)
        onExited: root.leave("dashboard")
        onClicked: NacrePanelState.openEdge("dashboard", root.screen.name)
    }
    NacreFrameLip {
        id: left
        decorationEnabled: false
        objectName: "frameLeftLip"
        active: root.visibilities.left
        edge: "left"
        base: NacreFrame.left
        x: root.leftRect.x
        y: root.leftRect.y
        width: root.leftRect.width
        height: root.leftRect.height
        visible: NacrePearls.drag?.kind !== "ai" && !NacrePanelState.hidden && width > 0 && DesktopSettings.data.leftDrawer !== false
        onEntered: buttons => root.approach("left", this, buttons)
        onMoved: buttons => root.approach("left", this, buttons)
        onExited: root.leave("left")
        onClicked: NacrePanelState.openEdge("left", root.screen.name)
    }
    NacreFrameLip {
        id: right
        decorationEnabled: false
        objectName: "frameRightLip"
        active: root.visibilities.osd
        edge: "right"
        base: NacreFrame.right
        x: root.rightRect.x
        y: root.rightRect.y
        width: root.rightRect.width
        height: root.rightRect.height
        visible: NacrePearls.drag?.kind !== "controls" && !NacrePanelState.hidden && width > 0 && DesktopSettings.data.rightEdge !== false
        onEntered: buttons => root.approach("osd", this, buttons)
        onMoved: buttons => root.approach("osd", this, buttons)
        onExited: root.leave("osd")
        onClicked: NacrePanelState.openControls("home", root.screen.name)
    }
}
