import QtQuick
import QtQuick.Controls.Basic

Slider {
    id: root
    property string icon: ""
    property string accessibleName: ""
    orientation: Qt.Vertical
    from: 0
    to: 1
    live: true
    padding: 0
    implicitWidth: 30
    implicitHeight: 120
    Accessible.name: accessibleName
    background: NacreSurface {
        x: root.leftPadding
        y: root.topPadding
        width: root.availableWidth
        height: root.availableHeight
        radius: Math.min(width, height) / 2
        color: NacreTokens.raised
        border.width: root.visualFocus ? 1 : 0
        border.color: NacreTokens.outline
        NacreSurface {
            width: parent.width
            height: parent.height * root.position
            anchors.bottom: parent.bottom
            radius: parent.radius
            color: NacreTokens.accent
            opacity: root.enabled ? 1 : 0.4
        }
        NacreIcon {
            text: root.icon
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.bottom: parent.bottom
            anchors.bottomMargin: 5
            font.pointSize: Math.max(1, Math.min(15, (parent.width - 8) * 0.75))
            color: root.position * parent.height > height + 5 ? NacreTokens.focusInk(NacreTokens.accent) : NacreTokens.ink
            visible: !!root.icon && parent.width >= 20 && parent.height >= 30
        }
    }
    handle: Item {
        width: 0
        height: 0
    }
}
