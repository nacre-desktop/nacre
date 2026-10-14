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
    property string peekName: ""
    property real peekCenter: 0
    property url peekSource: Qt.resolvedUrl("NacreStatusPeek.qml")
    function clearPeek() {
        peekDelay.stop();
        peekName = "";
        peekLoader.source = "";
    }
    function requestPeek(name, item, hovered) {
        if (name === "recording")
            return;
        if (!hovered) {
            if (peekName === name)
                clearPeek();
            return;
        }
        if (visibility?.launcher || visibility?.session)
            return;
        clearPeek();
        hoverHint = "";
        peekName = name;
        peekCenter = item.mapToItem(root, item.width / 2, item.height / 2).x;
        peekDelay.restart();
    }
    function openStatus(name, item) {
        clearPeek();
        hoverHint = "";
        if (name === "recording") {
            NacreMenu.workflows.work("record-stop");
            return;
        }
        if (name === "notifications") {
            const p = item.mapToItem(root, item.width / 2, item.height / 2);
            NacrePanelState.popout("notifications", p.x, screen.name);
            if (panel)
                panel.popouts.pinned = true;
            return;
        }
        const pearl = NacrePearls.entries.find(entry => entry.kind === "controls");
        if (pearl?.window?.bodyItem?.item?.navigate) {
            pearl.window.bodyItem.item.navigate(name);
            pearl.window.activate();
        } else
            NacrePanelState.openControls(name, screen.name, true);
    }
    onVisibleChanged: if (!visible)
        clearPeek()
    Timer {
        id: peekDelay
        interval: 400
        onTriggered: if (root.peekName && !NacrePanelState.hidden)
            peekLoader.setSource(root.peekSource, {
                output: root.screen,
                kind: root.peekName,
                center: root.peekCenter
            })
    }
    Loader {
        id: peekLoader
        objectName: "nacreStatusPeekLoader"
    }
    Connections {
        target: NacrePanelState
        function onHiddenChanged() {
            if (NacrePanelState.hidden)
                root.clearPeek();
        }
    }
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
        if (name === "recording") {
            hoverHint = status.recordingItem.label;
            hoverHintCenter = status.recordingItem.mapToItem(root, status.recordingItem.width / 2, status.recordingItem.height / 2).x;
            return;
        }
        rememberRegion(item);
        const pointer = item.mapToItem(root, item.mouseX ?? item.width / 2, item.mouseY ?? 0);
        NacreHoverIntent.observe(screen, pointer.x, pointer.y);
        if (!clicked && !NacreHoverIntent.canOpen("popouts", screen, item.pressedButtons ?? Qt.NoButton))
            return;
        if (!clicked) {
            if (visibility.osd)
                return;
            hoverHint = {
                audio: NacreI18n.tr("ui.header.hoverHint.audio", "Audio"),
                network: NacreI18n.tr("ui.header.hoverHint.wi-fi", "Wi-Fi"),
                bluetooth: NacreI18n.tr("ui.header.hoverHint.bluetooth", "Bluetooth"),
                battery: NacreI18n.tr("ui.header.hoverHint.power", "Power"),
                notifications: NacreI18n.tr("ui.header.hoverHint.notifications", "Notifications"),
                calendar: NacreI18n.tr("ui.header.hoverHint.calendar", "Calendar")
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
        if (status.recordingItem?.visible) {
            targets.push(status.recordingItem);
            names.push("recording");
        }
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
                accessibleName: NacreI18n.tr("topbar.header.accessibleName.open-applications", "Open applications")
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
                availableWidth: Math.max(170, Math.min(720, root.width / 2 - 130, root.width - right.width - 220))
                anchors.centerIn: parent
            }
        }
    }
    NacreActiveTitle {
        anchors.centerIn: parent
        width: Math.max(0, Math.min(650, root.width - 2 * Math.max(left.width, right.width) - 50))
        height: 30
        visible: width >= 100
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
        NacreSurface {
            id: updatesHolder
            objectName: "nacreStatusUpdates"
            implicitWidth: updates.implicitWidth
            implicitHeight: 34
            radius: 17
            color: updatesInput.hovered ? NacreColours.palette.m3surfaceContainerHighest : "transparent"
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
            NacreInteraction {
                id: updatesInput
                accessibleName: NacreI18n.tr("status.peek.updates", "Updates")
                function onClicked() {
                    root.openStatus("updates", updatesHolder);
                }
            }
            HoverHandler {
                blocking: false
                onHoveredChanged: root.requestPeek("updates", updatesHolder, hovered)
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
                onActivated: (name, item) => root.openStatus(name, item)
                onPeekRequested: (name, item, hovered) => root.requestPeek(name, item, hovered)
                onInteractionStarted: root.clearPeek()
            }
            Item {
                id: statusMouse
                objectName: "nacreHeaderStatusMouse"
                anchors.fill: parent
                readonly property bool containsMouse: statusHover.hovered
            }
            HoverHandler {
                id: statusHover
                blocking: false
                onHoveredChanged: {
                    NacreHoverIntent.popupHovered[root.screen.name] = hovered;
                    if (!hovered)
                        root.leavePopup();
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
        updatesItem: updatesHolder
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
                dashboardDepth: NacreHoverIntent.dashboardDepth,
                workspaceIds: workspaces.workspaceIds,
                workspaceStyle: workspaces.customIcons ? "icons" : "necklace",
                workspaceAppIcons: workspaces.iconAllocation,
                workspaceWindowHints: workspaces.showWindowHints ? "symbols" : "none",
                activeWorkspace: workspaces.activeCell?.workspaceId ?? -1,
                activeWorkspaceApps: workspaces.activeCell?.windowCount ?? 0
            });
        }
    }
}
