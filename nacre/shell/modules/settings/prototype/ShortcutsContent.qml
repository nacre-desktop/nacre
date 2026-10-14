pragma ComponentBehavior: Bound
import QtQuick
import qs.widgets
import qs.services

Column {
    id: root
    required property var registry
    spacing: NacreTokens.space.sm
    Component.onCompleted: DesktopExtras.request("keys", {})
    NacreText {
        width: parent.width
        visible: !DesktopExtras.keys.length
        text: DesktopExtras.message || root.registry.text("settings.registry.ui.shortcuts-empty")
        wrapMode: Text.Wrap
        color: NacreTokens.mutedInk
    }
    Repeater {
        model: DesktopExtras.keys
        delegate: Row {
            required property var modelData
            width: root.width
            NacreText {
                width: parent.width / 2
                text: parent.modelData.description || parent.modelData.name || ""
                wrapMode: Text.Wrap
            }
            NacreText {
                width: parent.width / 2
                text: parent.modelData.combo || parent.modelData.key || ""
                horizontalAlignment: Text.AlignRight
                font.family: NacreTokens.monoFamily
                wrapMode: Text.Wrap
            }
        }
    }
}
