import QtQuick
import qs.widgets

NacreSurface {
    id: root
    property string title: ""
    property string detail: ""
    property string icon: ""
    property string action: ""
    property bool selected: false
    signal activated
    width: parent.width
    height: 64
    radius: 12
    color: selected ? Qt.alpha(NacreTokens.accent, .12) : NacreTokens.body
    NacreIcon {
        x: 14
        anchors.verticalCenter: parent.verticalCenter
        text: root.icon
        font.pointSize: 17
        color: NacreTokens.accent
        visible: !!text
    }
    Column {
        x: root.icon ? 44 : 14
        y: 11
        width: parent.width - x - (root.action ? 105 : 14)
        spacing: 4
        NacreText {
            width: parent.width
            text: root.title
            font.pointSize: 11
            elide: Text.ElideRight
            textFormat: Text.PlainText
        }
        NacreText {
            width: parent.width
            text: root.detail
            color: NacreTokens.mutedInk
            font.pointSize: 9
            elide: Text.ElideRight
            textFormat: Text.PlainText
        }
    }
    ActionButton {
        anchors.right: parent.right
        anchors.rightMargin: 10
        anchors.verticalCenter: parent.verticalCenter
        text: root.action
        compact: true
        visible: !!text
        onClicked: root.activated()
    }
}
