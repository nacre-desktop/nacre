import QtQuick
import qs.widgets
import qs.services
import ".."

NacreExtraFrame {
    id: root
    title: NacreI18n.tr("dashboard.registry.network", "Network")
    description: NacreI18n.tr("dashboard.network-extra.description", "Your Wi-Fi connection and its recent traffic.")
    icon: "wifi"
    Column {
        anchors.fill: parent
        spacing: 14
        NacreDashboardNetworkCard {
            width: parent.width
            height: Math.max(170, parent.height - 130)
        }
        Row {
            width: parent.width
            spacing: 16
            NacreText {
                text: NacreNetwork.active ? NacreI18n.tr("dashboard.network-extra.signal", "{value}% signal", {
                    value: NacreNetwork.active.strength
                }) : NacreI18n.tr("dashboard.network-extra.disconnected", "Wi-Fi disconnected")
                font.pointSize: 11
            }
            NacreText {
                text: NacreNetwork.active ? NacreI18n.tr("dashboard.network-extra.frequency", "{value} MHz", {
                    value: NacreNetwork.active.frequency
                }) : ""
                color: NacreTokens.mutedInk
                font.pointSize: 11
            }
        }
        Row {
            spacing: 8
            ActionButton {
                text: NacreI18n.tr("dashboard.network-extra.refresh", "Refresh")
                compact: true
                onClicked: NacreNetwork.refresh()
            }
            ActionButton {
                text: NacreI18n.tr("dashboard.network-extra.settings", "Network settings")
                compact: true
                onClicked: NacrePanelState.openSettings("network")
            }
        }
    }
}
