import QtQuick
import qs.widgets
import qs.services

Column {

    spacing: 14
    NacreText {
        width: parent.width
        text: NacreI18n.tr("controlcenter.edit.hint", "Add the controls you use. Your everyday controls stay in place.")
        wrapMode: Text.Wrap
        color: NacreTokens.mutedInk
        font.pointSize: 11
    }
    Repeater {
        model: [
            {
                id: "airplane",
                label: NacreI18n.tr("controlcenter.overview.airplane", "Airplane mode"),
                icon: "flight"
            },
            {
                id: "vpn",
                label: NacreI18n.tr("controlcenter.overview.vpn", "VPN"),
                icon: "vpn_key"
            },
            {
                id: "record",
                label: NacreI18n.tr("controlcenter.overview.recorder", "Screen recorder"),
                icon: "videocam"
            },
            {
                id: "mic",
                label: NacreI18n.tr("controlcenter.overview.mic-mute", "Mic mute"),
                icon: "mic_off"
            }
        ]
        delegate: NacreControlTile {
            required property var modelData
            width: parent.width
            label: modelData.label
            icon: modelData.icon
            selected: NacreControlPreferences.optionalTiles.includes(modelData.id)
            detail: selected ? NacreI18n.tr("controlcenter.edit.added", "Added") : NacreI18n.tr("controlcenter.edit.add", "Add to controls")
            onClicked: NacreControlPreferences.toggle(modelData.id)
        }
    }
}
