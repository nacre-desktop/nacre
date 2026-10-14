import QtQuick

Item {
    id: root
    property real progress: 0
    property color ink: NacreTokens.accent
    readonly property real amount: Math.max(0, Math.min(1, progress))
    opacity: amount > 0 ? 1 : 0
    Accessible.ignored: true
    Behavior on opacity {
        NumberAnimation {
            duration: NacreTokens.motionEnabled ? 120 : 0
        }
    }
    Rectangle {
        anchors.fill: parent
        anchors.margins: -2
        radius: height / 2
        color: Qt.alpha(root.ink, .025 * root.amount)
    }
    Rectangle {
        anchors.fill: parent
        radius: height / 2
        gradient: Gradient {
            orientation: Gradient.Horizontal
            GradientStop {
                position: 0
                color: Qt.alpha(root.ink, .03 + .09 * root.amount)
            }
            GradientStop {
                position: 1
                color: Qt.alpha(NacreTokens.orientSecondary, .02 + .10 * root.amount)
            }
        }
        border.width: 1
        border.color: Qt.alpha(root.ink, .06 + .18 * root.amount)
    }
    Rectangle {
        objectName: "hoverRevealTrace"
        anchors.horizontalCenter: parent.horizontalCenter
        y: parent.height - 3
        width: Math.max(0, parent.width - 10) * root.amount
        height: 2
        radius: 1
        gradient: Gradient {
            orientation: Gradient.Horizontal
            GradientStop {
                position: 0
                color: root.ink
            }
            GradientStop {
                position: 1
                color: NacreTokens.orientSecondary
            }
        }
    }
}
