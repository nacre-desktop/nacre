import QtQuick
import qs.widgets

NacreSurface {
    id: root
    required property bool shouldUpdate
    property string title: ""
    property string description: ""
    property string icon: ""
    default property alias contents: body.data
    radius: 20
    color: NacreTokens.raised
    implicitWidth: 940
    implicitHeight: 562
    NacreIcon {
        x: 20
        y: 20
        text: root.icon
        font.pointSize: 23
        color: NacreTokens.accent
    }
    NacreText {
        x: 58
        y: 17
        width: parent.width - 80
        text: root.title
        font.pointSize: 24
        font.weight: Font.DemiBold
    }
    NacreText {
        x: 20
        y: 56
        width: parent.width - 40
        text: root.description
        color: NacreTokens.mutedInk
        font.pointSize: 10
        elide: Text.ElideRight
    }
    Item {
        id: body
        x: 20
        y: 88
        width: Math.max(0, parent.width - 40)
        height: Math.max(0, parent.height - 108)
        clip: true
    }
}
