import QtQuick

Item {
    id: root
    property string label: ""
    property bool checked: false
    signal toggled(bool value)
    implicitHeight: 34
    implicitWidth: caption.implicitWidth + 58
    NacreSurface {
        width: 42
        height: 24
        radius: 12
        anchors.verticalCenter: parent.verticalCenter
        color: root.checked ? NacreTokens.accent : NacreTokens.raised
        border.width: 1
        border.color: NacreTokens.outline
        Rectangle {
            x: root.checked ? 22 : 4
            y: 4
            width: 16
            height: 16
            radius: 8
            color: root.checked ? NacreTokens.focusInk(parent.color) : NacreTokens.mutedInk
        }
    }
    NacreText {
        id: caption
        x: 54
        anchors.verticalCenter: parent.verticalCenter
        text: root.label
        font.pointSize: 11
    }
    NacreInteraction {
        radius: 3
        accessibleName: root.label
        function onClicked() {
            root.toggled(!root.checked);
        }
    }
}
