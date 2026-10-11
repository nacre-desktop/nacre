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
    readonly property Item audioItem: speaker
    readonly property Item network: wifi
    readonly property Item bluetoothItem: bluetooth
    readonly property Item battery: batteryItem
    readonly property Item notificationsItem: notices
    readonly property var device: UPower.displayDevice
    readonly property bool batteryAvailable: device.ready && device.isLaptopBattery && Number.isFinite(device.percentage) && device.percentage >= 0 && device.percentage <= 1
    readonly property int batteryPercent: batteryAvailable ? Math.round(device.percentage * 100) : -1
    implicitWidth: Math.max(speaker.implicitWidth, wifi.implicitWidth, bluetooth.implicitWidth, batteryItem.implicitWidth, notices.implicitWidth)
    implicitHeight: speaker.implicitHeight + wifi.implicitHeight + bluetooth.implicitHeight + batteryItem.implicitHeight + notices.implicitHeight + NacreAppearance.spacing.small * 4
    Column {
        anchors.centerIn: parent
        spacing: NacreAppearance.spacing.small
        StatusTarget {
            id: speaker
            objectName: "nacreStatusAudio"
            icon: NacreAudio.muted ? "volume_off" : "volume_up"
            label: NacreAudio.muted ? "Sound muted" : "Sound"
            ink: root.colour
        }
        StatusTarget {
            id: wifi
            objectName: "nacreStatusNetwork"
            icon: NacreNetwork.active ? NacreIcons.getNetworkIcon(NacreNetwork.active.strength || 0) : "wifi_off"
            label: NacreNetwork.active ? "Wi-Fi connected" : "Wi-Fi disconnected"
            ink: root.colour
        }
        StatusTarget {
            id: bluetooth
            objectName: "nacreStatusBluetooth"
            icon: NacreBluetooth.powered ? "bluetooth" : "bluetooth_disabled"
            label: NacreBluetooth.powered ? "Bluetooth on" : "Bluetooth off"
            ink: root.colour
        }
        StatusTarget {
            id: batteryItem
            objectName: "nacreStatusBattery"
            icon: !root.batteryAvailable ? "battery_unknown" : UPower.onBattery ? "battery_full" : "battery_charging_full"
            label: root.batteryAvailable ? "Battery " + root.batteryPercent + "%" : "Battery unavailable"
            detail: root.batteryAvailable ? String(root.batteryPercent) : ""
            ink: root.batteryAvailable && root.batteryPercent <= 15 && UPower.onBattery ? NacreColours.palette.m3error : root.colour
        }
        StatusTarget {
            id: notices
            objectName: "nacreStatusNotifications"
            icon: "notifications"
            label: NacreNotifs.retained.length + " notifications"
            detail: String(Math.min(999, NacreNotifs.retained.length))
            ink: root.colour
        }
    }
    IpcHandler {
        target: "barStatus-" + root.screenName
        function state(): string {
            const items = [root.audioItem, root.network, root.bluetoothItem, root.battery, root.notificationsItem];
            const names = ["audio", "network", "bluetooth", "battery", "notifications"];
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
        required property string icon
        required property string label
        property string detail: ""
        required property color ink
        implicitWidth: root.horizontal ? content.implicitHeight : content.implicitWidth
        implicitHeight: root.horizontal ? content.implicitWidth : content.implicitHeight
        width: root.implicitWidth
        height: implicitHeight
        Accessible.role: Accessible.StaticText
        Accessible.name: label
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
    }
}
