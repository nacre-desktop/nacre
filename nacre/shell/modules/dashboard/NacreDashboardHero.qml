import QtQuick
import qs.widgets

NacreClip {
    id: root
    property color first: NacreTokens.accent
    property color second: NacreTokens.orientSecondary
    radius: 20
    color: NacreTokens.raised
    Rectangle {
        anchors.fill: parent
        color: "transparent"
        radius: root.radius
        z: 20
        border.width: 1
        border.color: Qt.alpha(root.first, .4)
    }
    Rectangle {
        anchors.top: parent.top
        visible: NacreTokens.gradientsEnabled
        anchors.left: parent.left
        anchors.right: parent.right
        height: 1
        gradient: Gradient {
            orientation: Gradient.Horizontal
            GradientStop {
                position: 0
                color: "transparent"
            }
            GradientStop {
                position: .3
                color: Qt.alpha(root.first, .6)
            }
            GradientStop {
                position: .7
                color: Qt.alpha(root.second, .6)
            }
            GradientStop {
                position: 1
                color: "transparent"
            }
        }
        z: 20
    }
}
