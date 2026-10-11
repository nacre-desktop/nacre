import QtQuick

Row {
    id: root
    property var tabs: []
    property string current: ""
    signal chosen(string name)
    spacing: 18
    height: 42
    Repeater {
        model: root.tabs
        Item {
            id: tab
            required property var modelData
            width: label.implicitWidth + 14
            height: 42
            NacreText {
                id: label
                anchors.centerIn: parent
                text: tab.modelData.name
                font.pointSize: 11
                color: root.current === tab.modelData.id ? NacreTokens.ink : NacreTokens.mutedInk
            }
            Rectangle {
                anchors.bottom: parent.bottom
                anchors.horizontalCenter: parent.horizontalCenter
                width: parent.width - 8
                height: 2
                radius: 1
                visible: root.current === tab.modelData.id
                color: NacreTokens.accent
            }
            NacreInteraction {
                radius: 4
                accessibleName: tab.modelData.name
                function onClicked() {
                    root.chosen(tab.modelData.id);
                }
            }
        }
    }
}
