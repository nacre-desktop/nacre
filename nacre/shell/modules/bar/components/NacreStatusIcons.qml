import QtQuick
import Quickshell.Io
import Quickshell.Services.UPower
import qs.widgets
import qs.services
import qs.config
import qs.utils

Item {
    id: root
    property string screenName: "unknown"
    property bool horizontal: false
    property color colour: NacreColours.palette.m3secondary
    readonly property Item recordingItem: recording
    readonly property Item audioItem: speaker
    readonly property Item network: wifi
    readonly property Item bluetoothItem: bluetooth
    readonly property Item battery: batteryItem
    readonly property Item notificationsItem: notices
    readonly property var device: UPower.displayDevice
    signal activated(string name, var item)
    signal peekRequested(string name, var item, bool hovered)
    signal interactionStarted
    function quickToggle(name) {
        if (name === "audio" && NacreAudio.available !== false)
            NacreAudio.toggleMute();
        else if (name === "network" && NacreNetwork.wifiAvailable !== false && !NacreControlActions.busy)
            NacreControlActions.request(["wifi-radio", NacreNetwork.wifiEnabled ? "off" : "on"]);
        else if (name === "bluetooth" && NacreBluetooth.available !== false && !NacreControlActions.busy)
            NacreControlActions.request(["bluetooth-power", NacreBluetooth.powered ? "off" : "on"]);
        else
            return false;
        interactionStarted();
        return true;
    }
    function adjust(name, direction) {
        if (!direction)
            return false;
        if (name === "audio" && NacreAudio.available !== false)
            NacreAudio.setVolume(Math.max(0, Math.min(1, NacreAudio.volume + Math.sign(direction) * .05)));
        else if (name === "battery") {
            const monitor = NacreBrightness.getMonitorForScreen({
                name: root.screenName
            });
            if (!monitor?.available)
                return false;
            monitor.setBrightness(Math.max(0, Math.min(1, monitor.brightness + Math.sign(direction) * .05)));
        } else
            return false;
        interactionStarted();
        return true;
    }
    readonly property bool batteryAvailable: !!device && device.ready && device.isPresent !== false && device.isLaptopBattery && Number.isFinite(device.percentage) && device.percentage >= 0 && device.percentage <= 1
    readonly property int batteryPercent: batteryAvailable ? Math.round(device.percentage * 100) : -1
    implicitWidth: Math.max(recording.visible ? recording.implicitWidth : 0, speaker.implicitWidth, wifi.implicitWidth, bluetooth.implicitWidth, batteryItem.implicitWidth, notices.implicitWidth)
    implicitHeight: (recording.visible ? recording.implicitHeight + NacreAppearance.spacing.small : 0) + speaker.implicitHeight + wifi.implicitHeight + bluetooth.implicitHeight + batteryItem.implicitHeight + notices.implicitHeight + NacreAppearance.spacing.small * 4
    Column {
        anchors.centerIn: parent
        spacing: NacreAppearance.spacing.small
        StatusTarget {
            id: recording
            key: "recording"
            objectName: "nacreStatusRecording"
            visible: NacreMenu.workflows?.data?.recording === true
            icon: "stop_circle"
            label: NacreI18n.tr("launcher.capture.stop-elapsed", "Stop recording {time}", {
                time: NacreMenu.workflows?.elapsed || "0:00"
            })
            detail: NacreMenu.workflows?.elapsed || "0:00"
            ink: NacreColours.palette.m3error
        }
        StatusTarget {
            id: speaker
            key: "audio"
            objectName: "nacreStatusAudio"
            icon: NacreAudio.muted ? "volume_off" : "volume_up"
            label: NacreAudio.muted ? NacreI18n.tr("components.status-icons.label.sound-muted", "Sound muted") : NacreI18n.tr("components.status-icons.label.sound", "Sound")
            ink: root.colour
        }
        StatusTarget {
            id: wifi
            key: "network"
            objectName: "nacreStatusNetwork"
            icon: NacreNetwork.active ? NacreIcons.getNetworkIcon(NacreNetwork.active.strength || 0) : "wifi_off"
            label: NacreNetwork.wifiAvailable === false ? NacreI18n.tr("ui.status-icons.label.no-wi-fi-adapter", "No Wi-Fi adapter") : NacreNetwork.active ? NacreI18n.tr("components.status-icons.label.wi-fi-connected", "Wi-Fi connected") : NacreI18n.tr("components.status-icons.label.wi-fi-disconnected", "Wi-Fi disconnected")
            ink: root.colour
        }
        StatusTarget {
            id: bluetooth
            key: "bluetooth"
            objectName: "nacreStatusBluetooth"
            icon: NacreBluetooth.powered ? "bluetooth" : "bluetooth_disabled"
            label: NacreBluetooth.available === false ? NacreI18n.tr("ui.status-icons.label.no-bluetooth-adapter", "No Bluetooth adapter") : NacreBluetooth.powered ? NacreI18n.tr("components.status-icons.label.bluetooth-on", "Bluetooth on") : NacreI18n.tr("components.status-icons.label.bluetooth-off", "Bluetooth off")
            ink: root.colour
        }
        StatusTarget {
            id: batteryItem
            key: "battery"
            objectName: "nacreStatusBattery"
            icon: !root.batteryAvailable ? "battery_unknown" : UPower.onBattery ? "battery_full" : "battery_charging_full"
            label: root.batteryAvailable ? NacreI18n.tr("ui.status-icons.label.battery-value0", "Battery {value0}%", {
                value0: root.batteryPercent
            }) : NacreI18n.tr("components.status-icons.label.battery-unavailable", "Battery unavailable")
            detail: root.batteryAvailable ? String(root.batteryPercent) : ""
            ink: root.batteryAvailable && root.batteryPercent <= 15 && UPower.onBattery ? NacreColours.palette.m3error : root.colour
        }
        StatusTarget {
            id: notices
            key: "notifications"
            objectName: "nacreStatusNotifications"
            icon: "notifications"
            label: NacreI18n.tr("ui.status-icons.label.value0-notifications", "{value0} notifications", {
                value0: NacreNotifs.retained.length
            })
            detail: String(Math.min(999, NacreNotifs.retained.length))
            ink: root.colour
        }
    }
    IpcHandler {
        target: "barStatus-" + root.screenName
        function state(): string {
            const items = [root.audioItem, root.network, root.bluetoothItem, root.battery, root.notificationsItem];
            const names = ["audio", "network", "bluetooth", "battery", "notifications"];
            if (recording.visible) {
                items.push(recording);
                names.push("recording");
            }
            return JSON.stringify({
                batteryAvailable: root.batteryAvailable,
                batteryPercent: root.batteryPercent,
                noticeCount: NacreNotifs.retained.length,
                frameSceneFocus: NacrePanelState.panels[root.screenName]?.input?.parent?.focus ?? false,
                frameSceneActiveFocus: NacrePanelState.panels[root.screenName]?.input?.parent?.activeFocus ?? false,
                targets: items.map((item, index) => {
                    const point = item.mapToItem(null, item.width / 2, item.height / 2);
                    return {
                        name: names[index],
                        x: point.x,
                        y: point.y
                    };
                })
            });
        }
    }
    component StatusTarget: Item {
        id: cell
        required property string key
        required property string icon
        required property string label
        property string detail: ""
        required property color ink
        implicitWidth: root.horizontal ? content.implicitHeight : content.implicitWidth
        implicitHeight: root.horizontal ? content.implicitWidth : content.implicitHeight
        width: root.implicitWidth
        height: implicitHeight
        property real radius: 12
        Row {
            id: content
            anchors.centerIn: parent
            rotation: root.horizontal ? 90 : 0
            spacing: 2
            NacreIcon {
                objectName: "nacreStatusGlyph"
                width: 24
                height: 28
                clip: true
                elide: Text.ElideRight
                horizontalAlignment: Text.AlignHCenter
                verticalAlignment: Text.AlignVCenter
                anchors.verticalCenter: parent.verticalCenter
                text: parent.parent.icon
                color: parent.parent.ink
            }
            NacreText {
                visible: text !== ""
                anchors.verticalCenter: parent.verticalCenter
                text: parent.parent.detail
                color: parent.parent.ink
                font.pointSize: 10
            }
        }
        NacreInteraction {
            objectName: cell.objectName + "Activation"
            accessibleName: cell.label
            function onClicked() {
                root.interactionStarted();
                if (cell.key === "recording") {
                    root.forceActiveFocus(Qt.OtherFocusReason);
                    NacreMenu.workflows?.work("record-stop");
                } else
                    root.activated(cell.key, cell);
            }
        }
        HoverHandler {
            blocking: false
            onHoveredChanged: root.peekRequested(cell.key, cell, hovered)
        }
        TapHandler {
            acceptedButtons: Qt.RightButton
            onTapped: root.quickToggle(cell.key)
        }
        WheelHandler {
            target: null
            enabled: ["audio", "battery"].includes(cell.key)
            onWheel: event => event.accepted = root.adjust(cell.key, event.angleDelta.y || event.pixelDelta.y)
        }
    }
}
