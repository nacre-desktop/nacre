import QtQuick
import Quickshell.Bluetooth
import Quickshell.Services.Pipewire
import qs.widgets
import qs.services

NacreExtraFrame {
    id: root
    readonly property var bluetoothDevices: [...Bluetooth.devices.values].filter(device => device.paired || device.connected).slice(0, 3)
    readonly property var audioDevices: [...Pipewire.nodes.values].filter(node => !node.isStream && !!node.audio).slice(0, 3)
    title: NacreI18n.tr("dashboard.registry.devices", "Devices")
    description: NacreI18n.tr("dashboard.devices-extra.description", "Bluetooth companions and your current audio routes.")
    icon: "devices"
    PwObjectTracker {
        objects: root.audioDevices
    }
    Row {
        anchors.fill: parent
        spacing: 14
        Column {
            width: (parent.width - 14) / 2
            spacing: 10
            NacreText {
                text: NacreI18n.tr("dashboard.devices-extra.bluetooth", "Bluetooth")
                font.pointSize: 13
                color: NacreTokens.accent
            }
            Repeater {
                model: root.bluetoothDevices
                NacreExtraRow {
                    required property var modelData
                    title: modelData.deviceName || modelData.name || modelData.address
                    detail: modelData.batteryAvailable ? NacreI18n.tr("dashboard.devices-extra.battery", "{value}% battery", {
                        value: Math.round(modelData.battery * 100)
                    }) : modelData.connected ? NacreI18n.tr("dashboard.devices-extra.connected", "Connected") : NacreI18n.tr("dashboard.devices-extra.saved", "Saved")
                    action: modelData.connected ? NacreI18n.tr("dashboard.devices-extra.disconnect", "Disconnect") : NacreI18n.tr("dashboard.devices-extra.connect", "Connect")
                    onActivated: DeviceActions.request(["bluetooth", modelData.connected ? "disconnect" : "connect", modelData.address])
                }
            }
            NacreText {
                width: parent.width
                visible: !root.bluetoothDevices.length
                text: NacreI18n.tr("dashboard.devices-extra.none", "No saved Bluetooth devices.")
                color: NacreTokens.mutedInk
                font.pointSize: 10
                wrapMode: Text.Wrap
            }
            ActionButton {
                text: NacreI18n.tr("dashboard.devices-extra.manage", "Manage devices")
                compact: true
                onClicked: NacrePanelState.openSettings("bluetooth")
            }
        }
        Column {
            width: (parent.width - 14) / 2
            spacing: 10
            NacreText {
                text: NacreI18n.tr("dashboard.devices-extra.audio", "Audio")
                font.pointSize: 13
                color: NacreTokens.accent
            }
            Repeater {
                model: root.audioDevices
                NacreExtraRow {
                    required property var modelData
                    title: modelData.description || modelData.name
                    detail: modelData.isSink ? NacreI18n.tr("dashboard.devices-extra.output", "Output") : NacreI18n.tr("dashboard.devices-extra.input", "Input")
                    selected: modelData.isSink ? Pipewire.defaultAudioSink === modelData : Pipewire.defaultAudioSource === modelData
                    action: NacreI18n.tr("dashboard.devices-extra.use", "Use")
                    onActivated: {
                        if (!modelData.ready)
                            return;
                        if (modelData.isSink)
                            Pipewire.preferredDefaultAudioSink = modelData;
                        else
                            Pipewire.preferredDefaultAudioSource = modelData;
                    }
                }
            }
            ActionButton {
                text: NacreI18n.tr("dashboard.devices-extra.audio-settings", "Audio settings")
                compact: true
                onClicked: NacrePanelState.openSettings("sound")
            }
        }
    }
}
