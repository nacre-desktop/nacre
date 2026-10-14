import QtQuick

Item {
    id: root
    required property string edge
    property real base: 10
    property real progress: 0
    property bool active: false
    property bool keyboardFocused: false
    readonly property real amount: Math.max(active || keyboardFocused ? 1 : 0, Math.min(1, progress))
    clip: true
    Accessible.ignored: true
    NacreIcon {
        objectName: "edgeTabArrow"
        text: root.edge === "left" ? root.active ? "chevron_left" : "chevron_right" : root.active ? "chevron_right" : "chevron_left"
        width: Math.max(0, Math.min(12, parent.width - 2))
        height: 20
        clip: true
        font.pointSize: Math.max(1, Math.min(10, root.width * .65))
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        x: (parent.width - width) / 2 + (root.edge === "left" ? .5 : -.5) * root.amount
        anchors.verticalCenter: parent.verticalCenter
        color: Qt.tint(NacreTokens.mutedInk, Qt.alpha(NacreTokens.accent, .35 + .65 * root.amount))
    }
    Rectangle {
        objectName: "edgeTabTrace"
        x: root.edge === "left" ? 0 : parent.width - 2
        anchors.verticalCenter: parent.verticalCenter
        width: 2
        height: Math.max(0, parent.height - 24) * root.amount
        radius: 1
        opacity: .25 + .65 * root.amount
        gradient: Gradient {
            GradientStop {
                position: 0
                color: NacreTokens.accent
            }
            GradientStop {
                position: 1
                color: NacreTokens.orientSecondary
            }
        }
    }
}
