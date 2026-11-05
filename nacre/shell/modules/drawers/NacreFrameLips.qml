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
    readonly property int hoverDelay: {
        const saved = DesktopSettings.data.lipHoverDelay;
        return Number.isInteger(saved) ? Math.max(0, Math.min(800, saved)) : 350;
    }
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
                left: leftRect,
                osd: rightRect
            }
        };
    }
    Component.onCompleted: publishRegions()
    onTopRectChanged: publishRegions()
    onLeftRectChanged: publishRegions()
    onRightRectChanged: publishRegions()
    property real nativeLeftProgress: 0
    property real nativeRightProgress: 0
    property real nativeDashboardProgress: 0
    property var nativeTriggeredEdges: ({})
    readonly property real dashboardProgress: NacreCompositor.externalEdgeSource ? nativeDashboardProgress : dashboardDwell.progress
    readonly property bool dashboardWaiting: NacreCompositor.externalEdgeSource ? nativeDashboardProgress > 0 : dashboardDwell.waiting
    function cancelDashboard() {
        dashboardDwell.cancel();
    }
    function settleDashboard() {
        if (!NacreHoverIntent.headers[screen.name])
            cancelDashboard();
    }
    function requestDashboard(buttons) {
        if (NacreCompositor.externalEdgeSource) {
            cancelDashboard();
            return;
        }
        if (buttons !== Qt.NoButton) {
            cancelDashboard();
            return;
        }
        if (visibilities.dashboard || DesktopSettings.data.clickEdgeMenus === true || !available || !NacreHoverIntent.canOpen("dashboard", screen, buttons) || dashboardWaiting)
            return;
        dashboardDwell.start();
    }
    function sideDwell(name) {
        return name === "left" ? leftDwell : rightDwell;
    }
    function cancelSides() {
        leftDwell.cancel();
        rightDwell.cancel();
    }
    function openSide(name, pointerOwned) {
        sideDwell(name).cancel();
        if (name === "osd" && visibilities.osd && visibilities.controlsMode === "dropdown") {
            NacrePanelState.openControls("home", screen.name, true, pointerOwned);
            return;
        }
        if (visibilities[name]) {
            NacreHoverIntent.dismiss(screen, pointerOwned);
            visibilities[name] = false;
            if (name === "left")
                visibilities.leftPinned = false;
            if (visibilities.edgeMenu === name)
                visibilities.edgeMenu = "";
            controller.settleHover();
        } else if (name === "left")
            NacrePanelState.openEdge(name, screen.name);
        else
            NacrePanelState.openControls("home", screen.name, false, pointerOwned);
    }
    onAvailableChanged: if (!available) {
        cancelDashboard();
        cancelSides();
        clearNativeCue("top");
        clearNativeCue("left");
        clearNativeCue("right");
        nativeTriggeredEdges = ({});
    }
    NacreHoverDwell {
        id: dashboardDwell
        objectName: "nacreDashboardDwell"
        duration: root.hoverDelay
        onTriggered: {
            if (!NacreHoverIntent.headers[root.screen.name] || !root.available || DesktopSettings.data.clickEdgeMenus === true || !NacreHoverIntent.canOpen("dashboard", root.screen, Qt.NoButton)) {
                root.cancelDashboard();
                return;
            }
            NacrePanelState.hoverEdge("dashboard", root.screen.name);
        }
    }
    NacreHoverDwell {
        id: leftDwell
        objectName: "nacreLeftDwell"
        duration: root.hoverDelay
        onTriggered: if (left.hovered && root.available && DesktopSettings.data.clickEdgeMenus !== true && NacreHoverIntent.canOpen("left", root.screen, Qt.NoButton))
            NacrePanelState.hoverEdge("left", root.screen.name)
    }
    NacreHoverDwell {
        id: rightDwell
        objectName: "nacreRightDwell"
        duration: root.hoverDelay
        onTriggered: if (right.hovered && root.available && DesktopSettings.data.clickEdgeMenus !== true && NacreHoverIntent.canOpen("osd", root.screen, Qt.NoButton))
            NacrePanelState.hoverEdge("osd", root.screen.name)
    }
    Connections {
        target: root.visibilities
        function onDashboardChanged() {
            if (!root.visibilities.dashboard)
                root.cancelDashboard();
        }
    }
    function approach(name, lip, buttons) {
        const point = lip.mapToItem(root, lip.pointerX, lip.pointerY);
        NacreHoverIntent.observe(screen, point.x, point.y);
        if (name === "dashboard") {
            NacreHoverIntent.setHeaderHover(screen.name, "lip", true);
            if (NacreCompositor.externalEdgeSource)
                return;
            requestDashboard(buttons);
            return;
        }
        if (NacreCompositor.externalEdgeSource)
            return;
        if (DesktopSettings.data.clickEdgeMenus === true || !available || !NacreHoverIntent.canOpen(name, screen, buttons))
            return;
        if ((!visibilities[name] || name === "osd" && visibilities.controlsMode === "dropdown") && !sideDwell(name).waiting)
            sideDwell(name).start();
    }
    function leave(name) {
        if (name === "dashboard") {
            NacreHoverIntent.setHeaderHover(screen.name, "lip", false);
            settleDashboard();
        }
        if (name !== "dashboard")
            sideDwell(name).cancel();
        NacreHoverIntent.rearm(name, screen);
        controller.settleHover();
    }
    function nativeEdgeName(edge) {
        return edge === "top" ? "dashboard" : edge === "left" ? "left" : edge === "right" ? "osd" : "";
    }
    function clearNativeCue(edge) {
        if (edge === "top") {
            nativeDashboardProgress = 0;
            NacreHoverIntent.setHeaderHover(screen.name, "lip", false);
        } else if (edge === "left") {
            nativeLeftProgress = 0;
        } else if (edge === "right") {
            nativeRightProgress = 0;
        }
    }
    Connections {
        target: NacreCompositor
        function onExternalEdgeSourceChanged() {
            if (NacreCompositor.externalEdgeSource) {
                root.cancelDashboard();
                root.cancelSides();
                root.nativeDashboardProgress = 0;
                root.nativeLeftProgress = 0;
                root.nativeRightProgress = 0;
                root.nativeTriggeredEdges = ({});
                NacreHoverIntent.setHeaderHover(root.screen.name, "lip", false);
            } else {
                root.nativeTriggeredEdges = ({});
                root.clearNativeCue("top");
                root.clearNativeCue("left");
                root.clearNativeCue("right");
                root.leave("dashboard");
                root.leave("left");
                root.leave("osd");
                if (root.available) {
                    if (top.hovered)
                        root.approach("dashboard", top, Qt.NoButton);
                    if (left.hovered)
                        root.approach("left", left, Qt.NoButton);
                    if (right.hovered)
                        root.approach("osd", right, Qt.NoButton);
                }
            }
        }
        function onEvent(value) {
            if (!NacreCompositor.externalEdgeSource || value?.output !== root.screen.name)
                return;
            if (value.type !== "edge-approach" && value.type !== "edge-hover" && value.type !== "edge-triggered" && value.type !== "edge-left")
                return;
            const position = value.position;
            if (!position || typeof position.length !== "number" || position.length !== 2 || !Number.isFinite(position[0]) || !Number.isFinite(position[1]))
                return;
            const edge = String(value.edge || "").toLowerCase();
            NacreHoverIntent.observe(root.screen, Number(position[0]), Number(position[1]));
            const name = root.nativeEdgeName(edge);
            if (!name)
                return;
            if (value.type === "edge-approach" || value.type === "edge-hover") {
                if (value.type === "edge-approach") {
                    const next = Object.assign({}, root.nativeTriggeredEdges);
                    delete next[edge];
                    root.nativeTriggeredEdges = next;
                }
                const progress = value.progress;
                const point = NacreHoverIntent.positions[root.screen.name];
                if (typeof progress !== "number" || !Number.isFinite(progress) || progress < 0 || progress > 1 || !root.available || NacreCompositor.overviewOpen || DesktopSettings.data.clickEdgeMenus === true || !NacreHoverIntent.canOpen(name, root.screen, Qt.NoButton) || !NacreHoverIntent.markedContains(name, point)) {
                    root.clearNativeCue(edge);
                    root.leave(name);
                    return;
                }
                if (value.type === "edge-hover" && root.nativeTriggeredEdges[edge] === true) {
                    root.clearNativeCue(edge);
                    if (edge === "top")
                        NacreHoverIntent.setHeaderHover(root.screen.name, "lip", true);
                    if (root.nativeTriggeredEdges[edge] === true && !root.visibilities[name])
                        NacrePanelState.hoverEdge(name, root.screen.name);
                    return;
                }
                if (edge === "top") {
                    root.nativeDashboardProgress = progress;
                    NacreHoverIntent.setHeaderHover(root.screen.name, "lip", true);
                } else if (edge === "left") {
                    root.nativeLeftProgress = progress;
                } else {
                    root.nativeRightProgress = progress;
                }
                return;
            }
            if (value.type === "edge-left") {
                const next = Object.assign({}, root.nativeTriggeredEdges);
                delete next[edge];
                root.nativeTriggeredEdges = next;
                root.clearNativeCue(edge);
                root.leave(name);
                return;
            }
            root.clearNativeCue(edge);
            const point = NacreHoverIntent.positions[root.screen.name];
            const canFire = root.available && !NacreCompositor.overviewOpen && DesktopSettings.data.clickEdgeMenus !== true && name && NacreHoverIntent.canOpen(name, root.screen, Qt.NoButton) && NacreHoverIntent.markedContains(name, point);
            const next = Object.assign({}, root.nativeTriggeredEdges);
            next[edge] = !!canFire;
            root.nativeTriggeredEdges = next;
            if (canFire)
                NacrePanelState.hoverEdge(name, root.screen.name);
        }
    }
    NacreFrameLip {
        id: top
        decorationEnabled: false
        progress: Math.max(root.focusProgress, root.nativeDashboardProgress)
        objectName: "frameTopLip"
        active: root.visibilities.dashboard
        edge: "top"
        x: root.topRect.x
        y: root.topRect.y
        width: root.topRect.width
        height: root.topRect.height
        visible: NacrePearls.drag?.kind !== "dashboard" && !NacrePanelState.hidden && width > 0 && DesktopSettings.data.dashboardEnabled !== false
        enabled: !NacreCompositor.externalEdgeSource
        onEntered: buttons => root.approach("dashboard", this, buttons)
        onMoved: buttons => root.approach("dashboard", this, buttons)
        onExited: root.leave("dashboard")
        onClicked: {
            root.cancelDashboard();
            NacrePanelState.openEdge("dashboard", root.screen.name);
        }
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
        visible: NacrePearls.drag?.kind !== "ai" && !NacrePanelState.hidden && width > 0 && DesktopSettings.data.aiSidebarEnabled !== false
        enabled: !NacreCompositor.externalEdgeSource
        onEntered: buttons => root.approach("left", this, buttons)
        onMoved: buttons => root.approach("left", this, buttons)
        onExited: root.leave("left")
        onClicked: root.openSide("left", pointerActivation)
        NacreEdgeTab {
            objectName: "frameLeftArrow"
            anchors.fill: parent
            edge: "left"
            base: NacreFrame.left
            progress: Math.max(leftDwell.progress, root.nativeLeftProgress)
            active: left.active
            keyboardFocused: left.activeFocus
        }
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
        visible: NacrePearls.drag?.kind !== "controls" && !NacrePanelState.hidden && width > 0 && DesktopSettings.data.controlCenterEnabled !== false
        enabled: !NacreCompositor.externalEdgeSource
        onEntered: buttons => root.approach("osd", this, buttons)
        onMoved: buttons => root.approach("osd", this, buttons)
        onExited: root.leave("osd")
        onClicked: root.openSide("osd", pointerActivation)
        NacreEdgeTab {
            objectName: "frameRightArrow"
            anchors.fill: parent
            edge: "right"
            base: NacreFrame.right
            progress: Math.max(rightDwell.progress, root.nativeRightProgress)
            active: right.active
            keyboardFocused: right.activeFocus
        }
    }
}
