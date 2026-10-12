import QtQuick
import qs.widgets
import qs.services

NacreSettingsPage {
    id: root
    property string page: "bluetooth"
    function power() {
        if (!DeviceActions.busy && NacreBluetooth.available)
            DeviceActions.request(["bluetooth-power", NacreBluetooth.powered ? "off" : "on"]);
    }
    function deviceAction(device, action) {
        if (DeviceActions.busy || !NacreBluetooth.available || !NacreBluetooth.devices.includes(device) || !["connect", "disconnect", "trust", "untrust"].includes(action))
            return;
        DeviceActions.request(["bluetooth-" + action, device.address]);
    }
    NacreSettingsSection {
        title: NacreI18n.tr("ui.bluetooth-page.title.bluetooth", "Bluetooth")
        description: !NacreBluetooth.available ? "No Bluetooth adapter" : NacreBluetooth.powered ? NacreI18n.tr("ui.bluetooth-page.description.bluetooth-is-on", "Bluetooth is on") : NacreI18n.tr("ui.bluetooth-page.description.bluetooth-is-off", "Bluetooth is off")
        Flow {
            width: parent.width
            spacing: 8
            ActionButton {
                text: NacreBluetooth.powered ? NacreI18n.tr("ui.bluetooth-page.text.turn-bluetooth-off", "Turn Bluetooth off") : NacreI18n.tr("ui.bluetooth-page.text.turn-bluetooth-on", "Turn Bluetooth on")
                enabled: !DeviceActions.busy && NacreBluetooth.available
                onClicked: root.power()
            }
            ActionButton {
                text: NacreI18n.tr("ui.bluetooth-page.text.pair-a-device", "Pair a device")
                enabled: NacreBluetooth.available
                onClicked: DesktopActions.execute("bluetooth")
            }
        }
    }
    NacreSettingsSection {
        title: NacreI18n.tr("ui.bluetooth-page.title.devices", "Devices")
        description: NacreI18n.tr("ui.bluetooth-page.description.pairing-and-discovery-open-in-the-bluetooth-manager", "Pairing and discovery open in the Bluetooth manager.")
        Repeater {
            model: NacreBluetooth.devices
            delegate: Column {
                required property var modelData
                width: parent.width
                spacing: 8
                NacreText {
                    width: parent.width
                    text: parent.modelData.alias || parent.modelData.name || parent.modelData.address
                    wrapMode: Text.Wrap
                }
                NacreText {
                    width: parent.width
                    text: (parent.modelData.connected ? NacreI18n.tr("ui.bluetooth-page.text.connected", "Connected") : parent.modelData.paired ? NacreI18n.tr("ui.bluetooth-page.text.paired", "Paired") : NacreI18n.tr("ui.bluetooth-page.text.not-paired", "Not paired")) + (parent.modelData.trusted ? NacreI18n.tr("ui.bluetooth-page.text.trusted", " · Trusted") : "")
                    font.pointSize: 10
                    color: NacreTokens.mutedInk
                }
                Flow {
                    width: parent.width
                    spacing: 8
                    ActionButton {
                        text: parent.parent.modelData.connected ? NacreI18n.tr("ui.bluetooth-page.text.disconnect", "Disconnect") : NacreI18n.tr("ui.bluetooth-page.text.connect", "Connect")
                        enabled: !DeviceActions.busy && NacreBluetooth.powered
                        onClicked: root.deviceAction(parent.parent.modelData, parent.parent.modelData.connected ? "disconnect" : "connect")
                    }
                    ActionButton {
                        text: parent.parent.modelData.trusted ? NacreI18n.tr("ui.bluetooth-page.text.remove-trust", "Remove trust") : NacreI18n.tr("ui.bluetooth-page.text.trust", "Trust")
                        enabled: !DeviceActions.busy && NacreBluetooth.available
                        onClicked: root.deviceAction(parent.parent.modelData, parent.parent.modelData.trusted ? "untrust" : "trust")
                    }
                }
            }
        }
        NacreText {
            width: parent.width
            visible: NacreBluetooth.devices.length === 0
            text: !NacreBluetooth.available ? "No Bluetooth adapter" : NacreI18n.tr("ui.bluetooth-page.text.no-devices-saved-open-the-bluetooth-manager-to-pair-one", "No devices saved. Open the Bluetooth manager to pair one.")
            wrapMode: Text.Wrap
            color: NacreTokens.mutedInk
        }
    }
    NacreText {
        width: parent.width
        text: DeviceActions.message
        wrapMode: Text.Wrap
        color: NacreTokens.mutedInk
    }
}
