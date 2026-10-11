import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic
import Quickshell.Services.UPower
import qs.widgets
import qs.services
import qs.config
import qs.modules.bar.popouts as Details
import qs.modules.notifications

FocusScope {
    id: root
    required property var screen
    required property bool visibility
    property bool keyboardActive: false
    property string section: "home"
    signal sectionRequested(string section)
    property real presentedWidth: visibility ? contentWidth : 0
    readonly property real contentWidth: Math.max(240, Math.min(424, parent?.width ?? 424))
    readonly property real contentHeight: Math.max(240, Math.min(860, parent?.height ?? 860))
    implicitWidth: Math.min(contentWidth, presentedWidth)
    implicitHeight: contentHeight
    visible: width > 0
    enabled: visibility
    clip: true
    function syncSize() {
        presentedWidth = visibility ? contentWidth : 0;
    }
    function focusExplicit() {
        if (visibility && keyboardActive)
            forceActiveFocus(Qt.OtherFocusReason);
    }
    onKeyboardActiveChanged: Qt.callLater(focusExplicit)
    onVisibilityChanged: {
        syncSize();
        Qt.callLater(focusExplicit);
    }
    onContentWidthChanged: syncSize()
    Component.onCompleted: {
        syncSize();
        Qt.callLater(focusExplicit);
    }
    Behavior on presentedWidth {
        enabled: NacreTokens.motionEnabled
        NumberAnimation {
            id: revealMotion
            duration: 280
            easing.type: Easing.OutCubic
        }
    }
    Connections {
        target: NacreTokens
        function onMotionEnabledChanged() {
            if (!NacreTokens.motionEnabled) {
                revealMotion.stop();
                root.syncSize();
            }
        }
    }
    Loader {
        active: root.visibility || root.presentedWidth > 0
        width: root.contentWidth
        height: root.contentHeight
        sourceComponent: Item {
            id: content
            readonly property real inset: 18
            Flickable {
                id: primaryScroll
                objectName: "controlPrimaryScroll"
                x: content.inset
                y: 18
                width: parent.width - content.inset * 2
                height: Math.min(fixed.height, Math.max(160, parent.height - 170))
                contentHeight: fixed.height
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: NacreScrollBar {}
                FastScroll {
                    view: primaryScroll
                }
                Column {
                    id: fixed
                    width: primaryScroll.width
                    spacing: 12
                    RowLayout {
                        width: parent.width
                        NacreText {
                            text: "Controls"
                            font.pointSize: 18
                            Layout.fillWidth: true
                        }
                        ActionButton {
                            text: "Settings"
                            icon: "settings"
                            compact: true
                            onClicked: NacrePanelState.openSettings("")
                        }
                    }
                    NacreControlLevels {
                        width: parent.width
                        monitor: NacreBrightness.getMonitorForScreen(root.screen)
                    }
                    GridLayout {
                        width: parent.width
                        columns: width < 310 ? 2 : 3
                        columnSpacing: 8
                        rowSpacing: 8
                        NacreControlTile {
                            objectName: "controlWifi"
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                            label: "Wi-Fi"
                            icon: "wifi"
                            detail: NacreNetwork.active?.ssid || (NacreNetwork.wifiEnabled ? "Not connected" : "Off")
                            selected: root.section === "network"
                            onClicked: root.sectionRequested(root.section === "network" ? "home" : "network")
                        }
                        NacreControlTile {
                            objectName: "controlBluetooth"
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                            label: "Bluetooth"
                            icon: "bluetooth"
                            detail: NacreBluetooth.powered ? "On" : "Off"
                            selected: root.section === "bluetooth"
                            onClicked: root.sectionRequested(root.section === "bluetooth" ? "home" : "bluetooth")
                        }
                        NacreControlTile {
                            objectName: "controlPower"
                            visible: NacreControlTools.data.powerProfilesSupported === true
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                            label: "Power"
                            icon: "battery_full"
                            detail: PowerProfiles.profile === PowerProfile.PowerSaver ? "Saver" : PowerProfiles.profile === PowerProfile.Performance ? "Performance" : "Balanced"
                            selected: root.section === "battery"
                            onClicked: root.sectionRequested(root.section === "battery" ? "home" : "battery")
                        }
                        NacreControlTile {
                            objectName: "controlDnd"
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                            label: "DND"
                            icon: "do_not_disturb_on"
                            detail: NacreNotifs.dnd ? "On" : "Off"
                            selected: NacreNotifs.dnd
                            onClicked: DesktopSettings.set("dnd", !NacreNotifs.dnd)
                        }
                        NacreControlTile {
                            objectName: "controlNightLight"
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                            label: "Night light"
                            icon: "nightlight"
                            detail: NacreControlTools.data.nightLightExternal ? "External" : NacreControlTools.data.nightLightEnabled ? "Warm" : "Off"
                            visible: NacreControlTools.data.nightLightSupported === true
                            enabled: !NacreControlTools.busy && !NacreControlTools.data.nightLightExternal
                            selected: NacreControlTools.data.nightLightEnabled === true
                            onClicked: NacreControlTools.toggleNightLight()
                        }
                        NacreControlTile {
                            objectName: "controlSound"
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                            label: "Devices"
                            icon: "speaker"
                            detail: "Audio output"
                            selected: root.section === "audio"
                            onClicked: root.sectionRequested(root.section === "audio" ? "home" : "audio")
                        }
                    }
                    GridLayout {
                        width: parent.width
                        columns: width < 340 ? 2 : 4
                        columnSpacing: 6
                        rowSpacing: 6
                        ActionButton {
                            text: "Capture"
                            icon: "screenshot"
                            compact: true
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                            enabled: NacreControlTools.data.screenshot === true
                            onClicked: NacreControlTools.capture("screenshot")
                        }
                        ActionButton {
                            text: "Clipboard"
                            compact: true
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                            onClicked: NacrePanelState.openMode("clipboard")
                        }
                        ActionButton {
                            text: "Colors"
                            icon: "palette"
                            compact: true
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                            onClicked: NacrePanelState.openMode("palette")
                        }
                        ActionButton {
                            text: "Picker"
                            icon: "colorize"
                            compact: true
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                            enabled: NacreControlTools.data.colorPicker === true
                            onClicked: NacreControlTools.capture("color-picker")
                        }
                    }
                    NacreText {
                        width: parent.width
                        text: NacreControlTools.error
                        visible: text !== ""
                        font.pointSize: 10
                        maximumLineCount: 2
                        wrapMode: Text.Wrap
                        color: NacreColours.palette.m3error
                    }
                }
            }
            RowLayout {
                id: detailsHeading
                x: content.inset
                y: primaryScroll.y + primaryScroll.height + 10
                width: parent.width - content.inset * 2
                NacreText {
                    Layout.fillWidth: true
                    text: root.section === "home" || root.section === "notifications" ? "Notifications" : "Connection details"
                    font.pointSize: 11
                    color: NacreTokens.mutedInk
                }
                ActionButton {
                    text: root.section === "home" || root.section === "notifications" ? "Clear" : "Back"
                    compact: true
                    onClicked: {
                        if (root.section === "home" || root.section === "notifications")
                            NacreNotifs.clearHistory();
                        else
                            root.sectionRequested("home");
                    }
                }
            }
            Flickable {
                id: detailsScroll
                objectName: "controlDetailsScroll"
                x: content.inset
                y: detailsHeading.y + detailsHeading.height + 8
                width: parent.width - content.inset * 2
                height: Math.max(0, parent.height - y - 18)
                contentHeight: detailsLoader.item?.implicitHeight ?? 0
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: NacreScrollBar {}
                FastScroll {
                    id: detailMotion
                    view: detailsScroll
                }
                Loader {
                    id: detailsLoader
                    width: detailsScroll.width
                    sourceComponent: root.section === "network" ? network : root.section === "bluetooth" ? bluetooth : root.section === "audio" ? audio : root.section === "battery" ? battery : history
                    onLoaded: {
                        detailMotion.cancel();
                        detailsScroll.contentY = 0;
                        if (root.section === "network")
                            NacreNetwork.refresh();
                    }
                }
            }
            Component {
                id: network
                Details.NacreNetworkPopup {
                    width: detailsScroll.width
                }
            }
            Component {
                id: bluetooth
                Details.NacreBluetoothPopup {
                    width: detailsScroll.width
                }
            }
            Component {
                id: audio
                Details.NacreSoundPopup {
                    width: detailsScroll.width
                }
            }
            Component {
                id: battery
                Details.NacreBatteryPopup {
                    profilesEnabled: NacreControlTools.data.powerProfilesSupported === true
                    width: detailsScroll.width
                }
            }
            Component {
                id: history
                Column {
                    width: detailsScroll.width
                    spacing: 8
                    NacreText {
                        width: parent.width
                        text: "No notifications"
                        visible: NacreNotifs.retained.length === 0
                        color: NacreTokens.mutedInk
                    }
                    Repeater {
                        model: NacreNotifs.retained
                        delegate: NacreNotice {
                            width: detailsScroll.width
                            history: true
                        }
                    }
                }
            }
        }
    }
}
