import QtQuick
import qs.widgets
import qs.services

Item {
    id: root
    readonly property var nearby: NacreNetwork.visibleNetworks
    implicitWidth: 352
    implicitHeight: body.implicitHeight
    function activate(point) {
        const current = nearby.find(entry => point?.bssid && entry.bssid === point.bssid);
        if (DeviceActions.busy || !NacreNetwork.wifiEnabled || !current)
            return false;
        if (current.active && NacreNetwork.wifiInterface)
            DeviceActions.request(["wifi-disconnect", NacreNetwork.wifiInterface]);
        else if (current.ssid)
            DeviceActions.connectWifi(current.ssid);
        else
            return false;
        return true;
    }
    Column {
        id: body
        width: root.width
        spacing: 10
        Row {
            width: parent.width
            spacing: 12
            NacreText {
                width: Math.max(0, parent.width - radioButton.width - parent.spacing)
                anchors.verticalCenter: parent.verticalCenter
                text: "Wi-Fi"
                font.pointSize: 14
            }
            ActionButton {
                id: radioButton
                objectName: "quickWifiPower"
                text: NacreNetwork.wifiEnabled ? "Turn off" : "Turn on"
                compact: true
                enabled: !DeviceActions.busy
                onClicked: if (enabled)
                    DeviceActions.request(["wifi-radio", NacreNetwork.wifiEnabled ? "off" : "on"])
            }
        }
        NacreText {
            width: parent.width
            text: NacreNetwork.active?.ssid || (NacreNetwork.wifiEnabled ? "Not connected" : "Wi-Fi is off")
            elide: Text.ElideRight
            color: NacreTokens.mutedInk
        }
        NacreQuickList {
            width: parent.width
            Column {
                width: parent.width
                spacing: 6
                Repeater {
                    model: root.nearby
                    delegate: NacreSurface {
                        required property var modelData
                        width: parent.width - 10
                        height: 48
                        radius: 12
                        color: modelData.active ? Qt.alpha(NacreTokens.accent, 0.12) : "transparent"
                        NacreText {
                            x: 10
                            width: Math.max(0, parent.width - connectionButton.width - 30)
                            height: parent.height
                            text: parent.modelData.ssid || "Hidden network"
                            verticalAlignment: Text.AlignVCenter
                            elide: Text.ElideRight
                        }
                        ActionButton {
                            id: connectionButton
                            objectName: parent.modelData.active ? "connectedWifiAction" : "availableWifiAction"
                            anchors.right: parent.right
                            anchors.rightMargin: 8
                            anchors.verticalCenter: parent.verticalCenter
                            text: parent.modelData.active ? "Disconnect" : "Connect"
                            compact: true
                            enabled: !DeviceActions.busy && NacreNetwork.wifiEnabled
                            onClicked: root.activate(parent.modelData)
                        }
                    }
                }
            }
        }
        NacreText {
            width: parent.width
            visible: root.nearby.length === 0 && NacreNetwork.wifiEnabled
            text: NacreNetwork.busy ? "Reading Wi-Fi state…" : "No nearby networks"
            color: NacreTokens.mutedInk
        }
        NacreText {
            width: parent.width
            visible: text !== ""
            text: NacreNetwork.error || NacreNetwork.monitorError || DeviceActions.message || ""
            maximumLineCount: 3
            elide: Text.ElideRight
            wrapMode: Text.WordWrap
            color: NacreTokens.mutedInk
        }
    }
}
