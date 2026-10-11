import QtQuick
import QtQuick.Layouts
import Quickshell.Io
import qs.widgets
import qs.services
import qs.config
import qs.modules.bar.components

Item {
    id: root
    required property var screen
    readonly property var visibility: NacrePanelState.screens[screen.name]
    readonly property var panel: NacrePanelState.panels[screen.name]
    readonly property bool clickMenus: DesktopSettings.data.clickEdgeMenus === true
    property string hoverHint: ""
    property real hoverHintCenter: 0
    property string registeredName: ""
    readonly property real dashboardHoverWidth: root.visibility?.dashboard ? Math.min(root.width, Math.max(208, root.panel?.dashboard?.width || 208)) : Math.min(208, root.width)
    readonly property bool openHeaderHovered: !!root.visibility?.dashboard && headerPointer.hovered && Math.abs(headerPointer.point.position.x - width / 2) <= dashboardHoverWidth / 2
    onOpenHeaderHoveredChanged: {
        NacreHoverIntent.setHeaderHover(screen.name, "open-bar", openHeaderHovered);
        root.panel?.input?.settleHover();
    }
    function registerOutput() {
        const name = screen?.name || "";
        if (name === registeredName)
            return;
        if (registeredName)
            NacreHoverIntent.release(registeredName, root);
        registeredName = name;
        if (name)
            NacreHoverIntent.register(name, root);
    }
    onScreenChanged: registerOutput()
    Component.onCompleted: registerOutput()
    Component.onDestruction: NacreHoverIntent.release(registeredName, root)
    HoverHandler {
        id: headerPointer
        blocking: false
        onPointChanged: NacreHoverIntent.observe(root.screen, point.position.x, point.position.y)
    }
    MouseArea {
        id: centerBand
        objectName: "nacreHeaderCenterBand"
        anchors.horizontalCenter: parent.horizontalCenter
        width: root.dashboardHoverWidth
        height: parent.height
        hoverEnabled: true
        acceptedButtons: Qt.LeftButton
        function approach(buttons) {
            NacreHoverIntent.observe(root.screen, x + mouseX, mouseY);
            NacreHoverIntent.setHeaderHover(root.screen.name, "bar", containsMouse);
            if (!root.clickMenus && root.visibility && !root.panel?.input?.modal && NacreHoverIntent.canOpen("dashboard", root.screen, buttons))
                NacrePanelState.hoverEdge("dashboard", root.screen.name);
        }
        onEntered: approach(pressedButtons)
        onPositionChanged: event => approach(event.buttons)
        onExited: {
            NacreHoverIntent.setHeaderHover(root.screen.name, "bar", false);
            NacreHoverIntent.rearm("dashboard", root.screen);
            root.panel?.input?.settleHover();
        }
        onClicked: NacrePanelState.openEdge("dashboard", root.screen.name)
    }
    Rectangle {
        anchors.fill: parent
        color: NacreTokens.body
    }
    function rememberRegion(item) {
        const p = item.mapToItem(root, 0, 0);
        NacreHoverIntent.recordPopupRegion(screen, p.x, p.y, item.width, item.height);
    }
    function showPopup(name, item, clicked) {
        if (!panel || !visibility || visibility.session || visibility.launcher || (visibility.dashboard && visibility.dashboardPinned))
            return;
        rememberRegion(item);
        const pointer = item.mapToItem(root, item.mouseX ?? item.width / 2, item.mouseY ?? 0);
        NacreHoverIntent.observe(screen, pointer.x, pointer.y);
        if (!clicked && !NacreHoverIntent.canOpen("popouts", screen, item.pressedButtons ?? Qt.NoButton))
            return;
        if (!clicked) {
            if (visibility.osd)
                return;
            hoverHint = {
                audio: "Audio",
                network: "Wi-Fi",
                bluetooth: "Bluetooth",
                battery: "Power",
                notifications: "Notifications",
                calendar: "Calendar"
            }[name] || name;
            const hintTarget = {
                audio: status.audioItem,
                network: status.network,
                bluetooth: status.bluetoothItem,
                battery: status.battery,
                notifications: status.notificationsItem
            }[name] || item;
            hoverHintCenter = hintTarget.mapToItem(root, hintTarget.width / 2, hintTarget.height / 2).x;
            return;
        }
        popupExpiry.stop();
        panel.popouts.headerHovered = true;
        const targets = {
            audio: status.audioItem,
            network: status.network,
            bluetooth: status.bluetoothItem,
            battery: status.battery,
            notifications: status.notificationsItem
        };
        const target = targets[name] || item;
        const p = target.mapToItem(root, target.width / 2, target.height / 2);
        if (!panel.popouts.hasCurrent || panel.popouts.currentName !== name)
            NacrePanelState.popout(name, p.x, screen.name);
    }
    function statusName(x) {
        const targets = [status.audioItem, status.network, status.bluetoothItem, status.battery, status.notificationsItem];
        const names = ["audio", "network", "bluetooth", "battery", "notifications"];
        let nearest = 0, distance = Infinity;
        for (let i = 0; i < targets.length; i++) {
            const p = targets[i].mapToItem(statusMouse, targets[i].width / 2, targets[i].height / 2);
            if (Math.abs(p.x - x) < distance) {
                distance = Math.abs(p.x - x);
                nearest = i;
            }
        }
        return names[nearest];
    }
    function leavePopup() {
        hoverHint = "";
        NacreHoverIntent.popupHovered[screen.name] = statusMouse.containsMouse || clockMouse.containsMouse;
        NacreHoverIntent.rearm("popouts", screen);
        if (panel)
            panel.popouts.headerHovered = statusMouse.containsMouse || clockMouse.containsMouse;
        popupExpiry.restart();
    }
    Timer {
        id: popupExpiry
        interval: 120
        onTriggered: {
            if (!root.panel)
                return;
            root.panel.popouts.headerHovered = statusMouse.containsMouse || clockMouse.containsMouse;
            if (!root.panel.popouts.headerHovered && !root.panel.popoutHovered && !root.panel.popouts.pinned)
                root.panel.popouts.hasCurrent = false;
        }
    }
    RowLayout {
        id: left
        anchors.left: parent.left
        anchors.leftMargin: 15
        anchors.verticalCenter: parent.verticalCenter
        spacing: 12
        BrandLogo {
            compact: true
            NacreInteraction {
                accessibleName: "Open applications"
                function onClicked() {
                    NacrePanelState.openMode("apps", "", false);
                }
            }
        }
        NacreSurface {
            radius: 17
            color: NacreColours.palette.m3surfaceContainer
            implicitWidth: workspaces.implicitWidth + 10
            implicitHeight: 34
            NacreWorkspaceRow {
                id: workspaces
                anchors.centerIn: parent
            }
        }
    }
    NacreActiveTitle {
        anchors.centerIn: parent
        width: Math.max(100, Math.min(650, root.width - 2 * Math.max(left.width, right.width) - 50))
        height: 30
        visible: true
        horizontal: true
        monitor: NacreBrightness.getMonitorForScreen(root.screen)
    }
    NacreStatusHint {
        output: root.screen
        text: root.hoverHint
        center: root.hoverHintCenter
    }
    RowLayout {
        id: right
        anchors.right: parent.right
        anchors.rightMargin: 15
        anchors.verticalCenter: parent.verticalCenter
        spacing: 12
        Item {
            implicitWidth: updates.implicitWidth
            implicitHeight: 34
            Row {
                id: updates
                anchors.centerIn: parent
                spacing: 4
                NacreIcon {
                    text: "package_2"
                    color: NacreColours.palette.m3primary
                }
                NacreText {
                    text: Updates.message || String(Updates.count)
                    color: NacreColours.palette.m3primary
                }
            }
            MouseArea {
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                onClicked: AppLaunch.run(["nacre-shell", "updates"])
            }
        }
        NacreSurface {
            id: statusHolder
            objectName: "nacreHeaderStatus"
            implicitWidth: status.implicitHeight + 20
            implicitHeight: 34
            radius: 17
            color: NacreColours.palette.m3surfaceContainer
            NacreStatusIcons {
                id: status
                anchors.centerIn: parent
                screenName: root.screen.name
                horizontal: true
                rotation: -90
            }
            MouseArea {
                id: statusMouse
                objectName: "nacreHeaderStatusMouse"
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onEntered: {
                    NacreHoverIntent.popupHovered[root.screen.name] = true;
                    root.showPopup(root.statusName(mouseX), this, false);
                }
                onPositionChanged: if (containsMouse)
                    root.showPopup(root.statusName(mouseX), this, false)
                onExited: root.leavePopup()
                onClicked: {
                    const name = root.statusName(mouseX);
                    root.hoverHint = "";
                    NacrePanelState.openControls(name, root.screen.name);
                }
            }
        }
        Item {
            id: clockHolder
            objectName: "nacreHeaderClock"
            implicitWidth: clock.implicitWidth
            implicitHeight: 34
            Row {
                id: clock
                anchors.centerIn: parent
                spacing: 7
                NacreIcon {
                    text: "calendar_month"
                    color: NacreColours.palette.m3tertiary
                }
                NacreText {
                    text: NacreTime.format("HH:mm")
                    color: NacreColours.palette.m3tertiary
                }
            }
            MouseArea {
                id: clockMouse
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onEntered: {
                    NacreHoverIntent.popupHovered[root.screen.name] = true;
                    root.showPopup("calendar", this, false);
                }
                onExited: root.leavePopup()
                onClicked: {
                    root.hoverHint = "";
                    root.showPopup("calendar", this, true);
                    if (root.panel && root.clickMenus)
                        root.panel.popouts.pinned = true;
                }
            }
        }
        NacrePowerButton {}
    }
    NacreHeaderForwarder {
        screen: root.screen
        statusItem: statusHolder
        calendarItem: clockHolder
    }
    IpcHandler {
        target: "header-" + root.screen.name
        function state(): string {
            return JSON.stringify({
                screen: root.screen.name,
                width: root.width,
                height: root.height,
                hovered: NacreHoverIntent.headers[root.screen.name] === true,
                pointerX: NacreHoverIntent.positions[root.screen.name]?.x ?? -1,
                pointerY: NacreHoverIntent.positions[root.screen.name]?.y ?? -1,
                buttons: 0,
                visibilityRegistered: !!root.visibility,
                dashboard: root.visibility?.dashboard ?? false,
                fullscreen: NacreHoverIntent.fullscreenFor(root.screen.name),
                blocked: NacreHoverIntent.blocked[root.screen.name] ?? {},
                headerFlag: NacreHoverIntent.headers[root.screen.name] ?? false,
                modalObserver: root.panel?.input?.modal === true,
                dashboardDepth: NacreHoverIntent.dashboardDepth
            });
        }
    }
}
