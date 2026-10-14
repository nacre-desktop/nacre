import QtQuick
import qs.widgets
import qs.services

Column {
    id: root
    spacing: 14
    Component.onCompleted: NacreVpn.refresh()
    NacreText {
        width: parent.width
        text: NacreI18n.tr("controlcenter.vpn.hint", "Connect to a saved VPN. Add or configure profiles in NetworkManager before using them here.")
        color: NacreTokens.mutedInk
        wrapMode: Text.Wrap
        font.pointSize: 11
    }
    Repeater {
        model: NacreVpn.profiles
        delegate: NacreControlTile {
            required property var modelData
            width: parent.width
            label: modelData.name
            icon: "vpn_key"
            detail: modelData.active ? NacreI18n.tr("controlcenter.vpn.connected", "Connected") : NacreI18n.tr("controlcenter.control-center.detail.off", "Off")
            selected: modelData.active
            available: !NacreVpn.busy
            onClicked: NacreVpn.activate(modelData.uuid, !modelData.active)
        }
    }
    NacreText {
        width: parent.width
        visible: text !== ""
        text: NacreVpn.error || (NacreVpn.busy ? NacreI18n.tr("controlcenter.vpn.working", "Updating VPN connections…") : !NacreVpn.profiles.length ? NacreI18n.tr("controlcenter.vpn.none", "No saved VPN profiles found.") : "")
        color: NacreTokens.mutedInk
        wrapMode: Text.Wrap
        font.pointSize: 11
    }
}
