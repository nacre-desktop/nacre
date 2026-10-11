import QtQuick
import qs.widgets
import qs.services

NacreSettingsPage {
    id: root
    property string page: "bluetooth"
    function power() {
        if (!DeviceActions.busy)
            DeviceActions.request(["bluetooth-power", NacreBluetooth.powered ? "off" : "on"]);
    }
    function deviceAction(device, action) {
        if (DeviceActions.busy || !NacreBluetooth.devices.includes(device) || !["connect", "disconnect", "trust", "untrust"].includes(action))
            return;
        DeviceActions.request(["bluetooth-" + action, device.address]);
    }
    NacreSettingsSection {
        title: "Bluetooth"
        description: NacreBluetooth.powered ? "Bluetooth is on" : "Bluetooth is off"
        Flow {
            width: parent.width
            spacing: 8
            ActionButton {
                text: NacreBluetooth.powered ? "Turn Bluetooth off" : "Turn Bluetooth on"
                enabled: !DeviceActions.busy
                onClicked: root.power()
            }
            ActionButton {
                text: "Pair a device"
                onClicked: DesktopActions.execute("bluetooth")
            }
        }
    }
    NacreSettingsSection {
        title: "Devices"
        description: "Pairing and discovery open in the Bluetooth manager."
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
                    text: (parent.modelData.connected ? "Connected" : parent.modelData.paired ? "Paired" : "Not paired") + (parent.modelData.trusted ? " · Trusted" : "")
                    font.pointSize: 10
                    color: NacreTokens.mutedInk
                }
                Flow {
                    width: parent.width
                    spacing: 8
                    ActionButton {
                        text: parent.parent.modelData.connected ? "Disconnect" : "Connect"
                        enabled: !DeviceActions.busy && NacreBluetooth.powered
                        onClicked: root.deviceAction(parent.parent.modelData, parent.parent.modelData.connected ? "disconnect" : "connect")
                    }
                    ActionButton {
                        text: parent.parent.modelData.trusted ? "Remove trust" : "Trust"
                        enabled: !DeviceActions.busy
                        onClicked: root.deviceAction(parent.parent.modelData, parent.parent.modelData.trusted ? "untrust" : "trust")
                    }
                }
            }
        }
        NacreText {
            width: parent.width
            visible: NacreBluetooth.devices.length === 0
            text: "No devices saved. Open the Bluetooth manager to pair one."
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
