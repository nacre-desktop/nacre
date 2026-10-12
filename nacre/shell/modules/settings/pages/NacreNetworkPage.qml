import QtQuick
import qs.widgets
import qs.services

NacreSettingsPage {
    id: root
    Component.onCompleted: NacreNetwork.refresh()
    property string page: "network"
    function request(action, value) {
        if (!DeviceActions.busy && NacreNetwork.wifiAvailable)
            DeviceActions.request(value === undefined ? [action] : [action, value]);
    }
    function connect(network) {
        if (!DeviceActions.busy && NacreNetwork.wifiAvailable && NacreNetwork.wifiEnabled && NacreNetwork.visibleNetworks.includes(network) && !network.active && network.ssid)
            DeviceActions.connectWifi(network.ssid);
    }
    NacreSettingsSection {
        title: NacreI18n.tr("ui.network-page.title.wi-fi", "Wi-Fi")
        description: !NacreNetwork.wifiAvailable ? "No Wi-Fi adapter; wired connections remain available." : NacreNetwork.active ? "Connected to " + NacreNetwork.active.ssid : NacreNetwork.wifiEnabled ? NacreI18n.tr("ui.network-page.description.not-connected", "Not connected") : NacreI18n.tr("ui.network-page.description.wi-fi-is-off", "Wi-Fi is off")
        Flow {
            width: parent.width
            spacing: 8
            ActionButton {
                text: NacreNetwork.wifiEnabled ? NacreI18n.tr("ui.network-page.text.turn-wi-fi-off", "Turn Wi-Fi off") : NacreI18n.tr("ui.network-page.text.turn-wi-fi-on", "Turn Wi-Fi on")
                enabled: !DeviceActions.busy && NacreNetwork.wifiAvailable
                onClicked: root.request("wifi-radio", NacreNetwork.wifiEnabled ? "off" : "on")
            }
            ActionButton {
                text: NacreI18n.tr("ui.network-page.text.scan-networks", "Scan networks")
                enabled: !DeviceActions.busy && NacreNetwork.wifiEnabled
                onClicked: root.request("wifi-scan")
            }
            ActionButton {
                text: NacreI18n.tr("ui.network-page.text.disconnect", "Disconnect")
                visible: !!NacreNetwork.active
                enabled: !DeviceActions.busy && !!NacreNetwork.wifiInterface
                onClicked: root.request("wifi-disconnect", NacreNetwork.wifiInterface)
            }
            ActionButton {
                text: NacreI18n.tr("ui.network-page.text.connection-editor", "Connection editor")
                onClicked: DesktopActions.execute("network")
            }
        }
    }
    NacreSettingsSection {
        title: NacreI18n.tr("ui.network-page.title.available-networks", "Available networks")
        description: NacreI18n.tr("ui.network-page.description.passwords-are-entered-in-the-connection-window-when-nee", "Passwords are entered in the connection window when needed.")
        Repeater {
            model: NacreNetwork.visibleNetworks
            delegate: Row {
                required property var modelData
                width: parent.width
                spacing: 12
                NacreText {
                    width: Math.max(0, parent.width - connectButton.width - 64)
                    text: parent.modelData.ssid || NacreI18n.tr("ui.network-page.text.hidden-network", "Hidden network")
                    wrapMode: Text.Wrap
                    anchors.verticalCenter: parent.verticalCenter
                }
                NacreText {
                    width: 40
                    text: Math.round(parent.modelData.strength) + "%"
                    font.pointSize: 10
                    anchors.verticalCenter: parent.verticalCenter
                }
                ActionButton {
                    id: connectButton
                    text: parent.modelData.active ? NacreI18n.tr("ui.network-page.text.connected", "Connected") : NacreI18n.tr("ui.network-page.text.connect", "Connect")
                    selected: parent.modelData.active
                    enabled: !DeviceActions.busy && !parent.modelData.active && !!parent.modelData.ssid
                    onClicked: root.connect(parent.modelData)
                }
            }
        }
        NacreText {
            width: parent.width
            visible: NacreNetwork.visibleNetworks.length === 0
            text: !NacreNetwork.wifiAvailable ? "No Wi-Fi adapter; wired connections remain available." : NacreNetwork.wifiEnabled ? NacreI18n.tr("ui.network-page.text.no-networks-found-scan-to-try-again", "No networks found. Scan to try again.") : NacreI18n.tr("ui.network-page.text.turn-wi-fi-on-to-see-nearby-networks", "Turn Wi-Fi on to see nearby networks.")
            wrapMode: Text.Wrap
            color: NacreTokens.mutedInk
        }
    }
    NacreText {
        width: parent.width
        text: NacreNetwork.error || DeviceActions.message || NacreNetwork.monitorError || (NacreNetwork.busy ? NacreI18n.tr("ui.network-page.text.refreshing-networks", "Refreshing networks…") : "")
        wrapMode: Text.Wrap
        color: NacreTokens.mutedInk
    }
}
