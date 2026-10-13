import QtQuick
import qs.widgets
import qs.services

Item {
    id: root
    readonly property var nearby: NacreNetwork.visibleNetworks
    property real maximumListHeight: 210
    Component.onCompleted: {
        NacreNetwork.refresh();
        request("wifi-scan");
    }
    implicitWidth: 352
    implicitHeight: body.implicitHeight
    function request(action, value) {
        if (DeviceActions.busy || !NacreNetwork.wifiAvailable)
            return false;
        if (action === "wifi-scan" && !NacreNetwork.wifiEnabled)
            return false;
        if (action === "wifi-disconnect" && (!NacreNetwork.active || !value || value !== NacreNetwork.wifiInterface))
            return false;
        if (!["wifi-radio", "wifi-scan", "wifi-disconnect"].includes(action))
            return false;
        DeviceActions.request(value === undefined ? [action] : [action, value]);
        return true;
    }
    function activate(point) {
        const current = nearby.find(entry => point?.bssid && entry.bssid === point.bssid && entry.interface === point.interface);
        if (DeviceActions.busy || !NacreNetwork.wifiAvailable || !NacreNetwork.wifiEnabled || !current)
            return false;
        if (current.active)
            return request("wifi-disconnect", current.interface || NacreNetwork.wifiInterface);
        if (!current.ssid)
            DesktopActions.execute("network");
        else
            DeviceActions.connectWifi(current.ssid);
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
                width: Math.max(0, parent.width - radioButton.width - scanButton.width - 2 * parent.spacing)
                anchors.verticalCenter: parent.verticalCenter
                text: NacreI18n.tr("popouts.network-popup.text.wi-fi", "Wi-Fi")
                font.pointSize: 14
            }
            ActionButton {
                id: scanButton
                objectName: "quickWifiScan"
                text: NacreI18n.tr("ui.network-page.text.scan-networks", "Scan networks")
                compact: true
                enabled: !DeviceActions.busy && NacreNetwork.wifiAvailable && NacreNetwork.wifiEnabled
                onClicked: root.request("wifi-scan")
            }
            ActionButton {
                id: radioButton
                objectName: "quickWifiPower"
                text: NacreNetwork.wifiEnabled ? NacreI18n.tr("popouts.network-popup.text.turn-off", "Turn off") : NacreI18n.tr("popouts.network-popup.text.turn-on", "Turn on")
                compact: true
                enabled: !DeviceActions.busy && NacreNetwork.wifiAvailable
                onClicked: root.request("wifi-radio", NacreNetwork.wifiEnabled ? "off" : "on")
            }
        }
        NacreText {
            width: parent.width
            text: !NacreNetwork.wifiAvailable ? NacreI18n.tr("ui.network-popup.text.no-wi-fi-adapter-wired-connections-remain-available", "No Wi-Fi adapter; wired connections remain available.") : NacreNetwork.active?.ssid || (NacreNetwork.wifiEnabled ? NacreI18n.tr("popouts.network-popup.text.not-connected", "Not connected") : NacreI18n.tr("popouts.network-popup.text.wi-fi-is-off", "Wi-Fi is off"))
            elide: Text.ElideRight
            color: NacreTokens.mutedInk
        }
        NacreQuickList {
            objectName: "wifiNetworkList"
            maximumHeight: root.maximumListHeight
            width: parent.width
            Column {
                width: parent.width
                spacing: 6
                Repeater {
                    model: root.nearby
                    delegate: NacreSurface {
                        required property var modelData
                        width: parent.width - 10
                        height: Math.max(48, connectionButton.implicitHeight + 12)
                        radius: 12
                        color: modelData.active ? Qt.alpha(NacreTokens.accent, 0.12) : "transparent"
                        NacreText {
                            x: 10
                            width: Math.max(0, parent.width - connectionButton.width - 30)
                            height: parent.height
                            text: parent.modelData.ssid || NacreI18n.tr("popouts.network-popup.text.hidden-network", "Hidden network")
                            verticalAlignment: Text.AlignVCenter
                            elide: Text.ElideRight
                        }
                        ActionButton {
                            id: connectionButton
                            objectName: parent.modelData.active ? "connectedWifiAction" : "availableWifiAction"
                            anchors.right: parent.right
                            anchors.rightMargin: 8
                            anchors.verticalCenter: parent.verticalCenter
                            text: parent.modelData.active ? NacreI18n.tr("popouts.network-popup.text.disconnect", "Disconnect") : parent.modelData.ssid ? NacreI18n.tr("popouts.network-popup.text.connect", "Connect") : NacreI18n.tr("ui.network-page.text.connection-editor", "Connection editor")
                            compact: true
                            enabled: !DeviceActions.busy && NacreNetwork.wifiAvailable && NacreNetwork.wifiEnabled
                            onClicked: root.activate(parent.modelData)
                        }
                    }
                }
            }
        }
        NacreText {
            width: parent.width
            visible: root.nearby.length === 0 && NacreNetwork.wifiEnabled
            text: NacreNetwork.busy ? NacreI18n.tr("popouts.network-popup.text.reading-wi-fi-state", "Reading Wi-Fi state…") : NacreI18n.tr("popouts.network-popup.text.no-nearby-networks", "No nearby networks")
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
