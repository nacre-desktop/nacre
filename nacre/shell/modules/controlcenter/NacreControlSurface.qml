import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic
import Quickshell.Services.UPower
import qs.widgets
import qs.services

Item {
    id: content
    readonly property real inset: 18
    readonly property real preferredHeight: panel.section === "session" ? Math.max(300, page.y + page.contentHeight + 18) : 650
    RowLayout {
        id: status
        x: content.inset
        y: 14
        width: parent.width - content.inset * 2
        height: 32
        spacing: 3
        Item {
            Layout.fillWidth: true
            implicitHeight: 32
            Row {
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width
                spacing: 5
                NacreIcon {
                    id: batteryGlyph
                    text: UPower.onBattery ? "battery_full" : "battery_charging_full"
                    font.pointSize: 15
                    color: NacreTokens.accent
                }
                NacreText {
                    id: batteryPercent
                    text: panel.batteryAvailable ? NacreI18n.number(Math.round(UPower.displayDevice.percentage * 100)) + "%" : NacreI18n.tr("battery.connected-power", "Connected to power")
                    font.pointSize: 10
                    font.weight: Font.Medium
                }
                NacreText {
                    width: Math.max(0, parent.width - batteryGlyph.implicitWidth - batteryPercent.implicitWidth - parent.spacing * 2)
                    text: panel.batteryEstimate
                    font.pointSize: 9
                    color: NacreTokens.mutedInk
                    elide: Text.ElideRight
                }
            }
            NacreInteraction {
                accessibleName: NacreI18n.tr("controlcenter.footer.power-details", "Open power and battery details")
                function onClicked() {
                    panel.navigate("battery");
                }
            }
        }
        NacreControlButton {
            objectName: "controlEdit"
            icon: "edit"
            label: NacreI18n.tr("controlcenter.edit.button", "Edit")
            selected: panel.section === "edit"
            onClicked: panel.navigate("edit")
        }
        NacreControlButton {
            icon: "settings"
            label: NacreI18n.tr("controlcenter.control-center.text.settings", "Settings")
            onClicked: NacrePanelState.openSettings("")
        }
        NacreControlButton {
            objectName: "controlPower"
            selected: panel.section === "session"
            icon: "power_settings_new"
            label: NacreI18n.tr("controlcenter.footer.power", "Power menu")
            onClicked: panel.navigate("session")
        }
    }
    RowLayout {
        id: header
        x: content.inset
        y: status.y + status.height + 12
        width: parent.width - content.inset * 2
        height: Math.max(36, heading.implicitHeight)
        visible: panel.section !== "home"
        NacreControlButton {
            id: back
            objectName: "controlBack"
            icon: "arrow_back"
            label: NacreI18n.tr("controlcenter.control-center.text.back", "Back")
            onClicked: panel.navigate("home")
        }
        NacreText {
            id: heading
            Layout.fillWidth: true
            Layout.minimumWidth: 0
            text: panel.detailNames[panel.section] || ""
            font.pointSize: 18
            font.weight: Font.Medium
            wrapMode: Text.Wrap
        }
    }
    Connections {
        target: NacreTokens
        function onMotionEnabledChanged() {
            if (!NacreTokens.motionEnabled) {
                enter.complete();
                pageLoader.x = 0;
            }
        }
    }
    Flickable {
        id: page
        objectName: "controlPageScroll"
        x: content.inset
        y: header.visible ? header.y + header.height + 14 : status.y + status.height + 18
        width: parent.width - content.inset * 2
        height: parent.height - y - 18
        contentHeight: pageLoader.item?.implicitHeight || 0
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        flickableDirection: Flickable.VerticalFlick
        ScrollBar.vertical: NacreScrollBar {}
        FastScroll {
            view: page
        }
        Loader {
            id: pageLoader
            objectName: "controlPageLoader"
            width: page.width

            onLoaded: {
                page.contentY = 0;
                if (panel.section !== "home" && (panel.keyboardActive || panel.detached))
                    back.focusControl();
                enter.restart();
            }
        }
        NumberAnimation {
            id: enter
            target: pageLoader
            property: "x"
            from: NacreTokens.motionEnabled ? 22 : 0
            to: 0
            duration: NacreTokens.motionEnabled ? 180 : 0
            easing.type: Easing.OutCubic
        }
    }
    required property var panel
    function showPage() {
        const name = panel.section;
        const paths = {
            home: "NacreControlOverview.qml",
            edit: "NacreControlEdit.qml",
            network: "../bar/popouts/NacreNetworkPopup.qml",
            bluetooth: "../bar/popouts/NacreBluetoothPopup.qml",
            audio: "../bar/popouts/NacreSoundPopup.qml",
            battery: "NacreControlPower.qml",
            session: "NacreControlSession.qml",
            updates: "NacreControlUpdates.qml"
        };
        const properties = name === "home" ? {
            screen: panel.screen,
            active: panel.hostActive && panel.visibility
        } : ["audio", "session"].includes(name) ? {
            active: panel.hostActive && panel.visibility
        } : ["network", "bluetooth"].includes(name) ? {
            showHeading: false,
            maximumListHeight: Math.max(140, page.height - (name === "network" ? 180 : 130))
        } : !paths[name] ? {
            section: name,
            screen: panel.screen,
            active: panel.hostActive && panel.visibility
        } : {};
        pageLoader.source = "";
        pageLoader.setSource(Qt.resolvedUrl(paths[name] || "NacreControlDetail.qml"), properties);
    }
    Component.onCompleted: showPage()
    Connections {
        target: panel
        function onSectionChanged() {
            content.showPage();
        }
        function onScreenChanged() {
            content.showPage();
        }
    }
    Connections {
        target: pageLoader.item
        ignoreUnknownSignals: true
        function onDetailsRequested(name) {
            panel.navigate(name);
        }
    }
    Binding {
        target: pageLoader.item && pageLoader.item.maximumListHeight !== undefined ? pageLoader.item : null
        property: "maximumListHeight"
        value: Math.max(140, page.height - (panel.section === "network" ? 180 : 130))
        when: !!target
    }
    Binding {
        target: pageLoader.item && pageLoader.item.active !== undefined ? pageLoader.item : null
        property: "active"
        value: panel.hostActive && panel.visibility
        when: !!target
    }
    NacreSurface {
        objectName: "controlDetachedBackground"
        anchors.fill: parent
        visible: panel.detached
        radius: 18
        color: NacreTokens.body
        border.width: 0
        z: -2
    }
}
