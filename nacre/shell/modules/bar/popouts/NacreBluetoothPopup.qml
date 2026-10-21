import QtQuick
import QtQuick.Layouts
import qs.widgets
import qs.services
import qs.modules.controlcenter as Controls

Item {
    id: root
    property bool showHeading: true
    property bool embedded: false
    property real maximumListHeight: 420
    property bool discoveryOwned: false
    property var pairingDevice: null
    readonly property var known: NacreBluetooth.devices
    function scan() {
        if (!NacreBluetooth.adapter || !NacreBluetooth.powered)
            return;
        if (!NacreBluetooth.discovering) {
            discoveryOwned = true;
            NacreBluetooth.adapter.discovering = true;
            scanTimeout.restart();
        }
    }
    Component.onDestruction: if (discoveryOwned && NacreBluetooth.adapter)
        NacreBluetooth.adapter.discovering = false
    Timer {
        id: scanTimeout
        interval: 20000
        onTriggered: {
            if (root.discoveryOwned && NacreBluetooth.adapter)
                NacreBluetooth.adapter.discovering = false;
            root.discoveryOwned = false;
        }
    }
    implicitWidth: 352
    implicitHeight: body.implicitHeight
    function activate(device) {
        const current = known.find(entry => device?.address && entry.address === device.address);
        if (NacreControlActions.busy || pairingDevice || !NacreBluetooth.powered || !current)
            return false;
        if (!current.paired) {
            forceActiveFocus(Qt.OtherFocusReason);
            pairingDevice = current;
        } else
            NacreControlActions.request([current.connected ? "bluetooth-disconnect" : "bluetooth-connect", current.address]);
        return true;
    }
    Column {
        id: body
        width: root.width
        spacing: 10
        RowLayout {
            width: parent.width
            NacreText {
                Layout.fillWidth: true
                text: NacreI18n.tr("popouts.bluetooth-popup.text.bluetooth", "Bluetooth")
                font.pointSize: 13
                visible: root.showHeading
            }
            ActionButton {
                text: NacreBluetooth.discovering ? NacreI18n.tr("controlcenter.bluetooth.scanning", "Scanning…") : NacreI18n.tr("controlcenter.bluetooth.scan", "Find devices")
                icon: "search"
                compact: true
                enabled: NacreBluetooth.powered && !root.pairingDevice && !NacreBluetooth.discovering
                onClicked: root.scan()
            }
            Item {
                Layout.fillWidth: true
                visible: !root.showHeading
            }
            Controls.NacreControlSwitch {
                objectName: "quickBluetoothPower"
                label: NacreI18n.tr("controlcenter.bluetooth.radio", "Bluetooth radio")
                checked: NacreBluetooth.powered
                enabled: !NacreControlActions.busy && !root.pairingDevice && NacreBluetooth.available
                onClicked: if (enabled)
                    NacreControlActions.request(["bluetooth-power", NacreBluetooth.powered ? "off" : "on"])
            }
        }
        Loader {
            width: parent.width
            active: !!root.pairingDevice
            sourceComponent: Controls.NacreBluetoothPairing {
                address: root.pairingDevice?.address || ""
                deviceName: root.pairingDevice?.alias || root.pairingDevice?.name || ""
                onCancelled: root.pairingDevice = null
                onPaired: {
                    const device = NacreBluetooth.devices.find(value => value.address === root.pairingDevice?.address);
                    if (device?.native) {
                        device.native.trusted = true;
                        device.native.connect();
                    }
                    root.pairingDevice = null;
                    NacreBluetooth.refresh();
                }
            }
        }
        NacreQuickList {
            embedded: root.embedded
            width: parent.width
            maximumHeight: root.maximumListHeight
            Column {
                width: parent.width
                spacing: 6
                Repeater {
                    model: root.known
                    delegate: NacreSurface {
                        required property var modelData
                        width: parent.width - 10
                        height: 68
                        radius: 12
                        color: modelData.connected ? NacreColours.palette.m3primaryContainer : NacreTokens.raised
                        readonly property color ink: modelData.connected ? NacreColours.palette.m3onPrimaryContainer : NacreTokens.ink
                        NacreIcon {
                            x: 12
                            anchors.verticalCenter: parent.verticalCenter
                            text: /headset|headphone|audio/.test(parent.modelData.icon || "") ? "headphones" : /keyboard/.test(parent.modelData.icon || "") ? "keyboard" : /mouse/.test(parent.modelData.icon || "") ? "mouse" : /phone/.test(parent.modelData.icon || "") ? "smartphone" : "bluetooth"
                            font.pointSize: 18
                            color: parent.ink
                        }
                        NacreText {
                            x: 44
                            width: Math.max(0, parent.width - connectionButton.width - 64)
                            height: 36
                            text: parent.modelData.alias || parent.modelData.name || NacreI18n.tr("popouts.bluetooth-popup.text.bluetooth-device", "Bluetooth device")
                            verticalAlignment: Text.AlignVCenter
                            color: parent.ink
                            elide: Text.ElideRight
                        }
                        Row {
                            x: 44
                            y: 39
                            spacing: 5
                            NacreIcon {
                                visible: parent.parent.modelData.batteryAvailable === true
                                text: parent.parent.modelData.battery < .2 ? "battery_1_bar" : parent.parent.modelData.battery < .6 ? "battery_4_bar" : "battery_full"
                                font.pointSize: 11
                                color: parent.parent.ink
                            }
                            NacreText {
                                text: parent.parent.modelData.batteryAvailable ? NacreI18n.number(Math.round(parent.parent.modelData.battery * 100)) + "%" : parent.parent.modelData.connected ? NacreI18n.tr("controlcenter.bluetooth.connected", "Connected") : parent.parent.modelData.paired ? NacreI18n.tr("controlcenter.bluetooth.paired", "Paired") : NacreI18n.tr("controlcenter.bluetooth.new", "Ready to pair")
                                font.pointSize: 9
                                color: parent.parent.ink
                                opacity: .8
                                Accessible.name: parent.parent.modelData.batteryAvailable ? NacreI18n.tr("controlcenter.bluetooth.battery", "Battery {percent}%", {
                                    percent: NacreI18n.number(Math.round(parent.parent.modelData.battery * 100))
                                }) : text
                            }
                        }
                        ActionButton {
                            id: connectionButton
                            objectName: "quickBluetoothConnection"
                            anchors.right: parent.right
                            anchors.rightMargin: 8
                            anchors.verticalCenter: parent.verticalCenter
                            text: parent.modelData.connected ? NacreI18n.tr("popouts.bluetooth-popup.text.disconnect", "Disconnect") : parent.modelData.pairing ? NacreI18n.tr("controlcenter.bluetooth.pairing", "Pairing…") : parent.modelData.paired ? NacreI18n.tr("popouts.bluetooth-popup.text.connect", "Connect") : NacreI18n.tr("controlcenter.bluetooth.pair", "Pair")
                            compact: true
                            accessibleLabel: NacreI18n.tr("controlcenter.bluetooth.device-action", "{action} {name}", {
                                action: connectionButton.text,
                                name: parent.modelData.alias || parent.modelData.name
                            })
                            enabled: !NacreControlActions.busy && !root.pairingDevice && NacreBluetooth.powered && !parent.modelData.pairing
                            onClicked: root.activate(parent.modelData)
                        }
                    }
                }
            }
        }
        NacreText {
            width: parent.width
            visible: !NacreBluetooth.powered || root.known.length === 0
            text: !NacreBluetooth.available ? NacreI18n.tr("ui.bluetooth-popup.text.no-bluetooth-adapter", "No Bluetooth adapter") : NacreBluetooth.powered ? NacreI18n.tr("popouts.bluetooth-popup.text.no-known-devices", "No known devices") : NacreI18n.tr("popouts.bluetooth-popup.text.bluetooth-is-off", "Bluetooth is off")
            color: NacreTokens.mutedInk
        }
        NacreText {
            width: parent.width
            visible: text !== ""
            text: NacreControlActions.message || ""
            maximumLineCount: 3
            wrapMode: Text.WordWrap
            elide: Text.ElideRight
            color: NacreTokens.mutedInk
        }
    }
}
