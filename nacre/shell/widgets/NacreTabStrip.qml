import QtQuick
import qs.services

Row {
    id: root
    property var tabs: []
    property string current: ""
    signal chosen(string name)
    spacing: NacreTokens.space.xl
    height: 42
    Repeater {
        model: root.tabs
        Item {
            id: tab
            required property var modelData
            width: label.implicitWidth + NacreTokens.space.lg
            height: 42
            NacreText {
                id: label
                anchors.centerIn: parent
                text: NacreI18n.modelLabel(tab.modelData, "name")
                font.pointSize: NacreTokens.textSize.caption
                color: root.current === tab.modelData.id ? NacreTokens.ink : NacreTokens.mutedInk
            }
            Rectangle {
                anchors.bottom: parent.bottom
                anchors.horizontalCenter: parent.horizontalCenter
                width: parent.width - NacreTokens.space.sm
                height: 2
                radius: NacreTokens.radii.full
                visible: root.current === tab.modelData.id
                color: NacreTokens.accent
            }
            NacreInteraction {
                radius: NacreTokens.radii.full
                accessibleName: NacreI18n.modelLabel(tab.modelData, "name")
                function onClicked() {
                    root.chosen(tab.modelData.id);
                }
            }
        }
    }
}
