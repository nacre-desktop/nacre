import QtQuick

Item {
    id: root
    property real progress: 0
    property bool halo: true
    property bool outline: true
    property real radius: height / 2
    property real topLeftRadius: radius
    property real topRightRadius: radius
    property real bottomLeftRadius: radius
    property real bottomRightRadius: radius
    property real traceLeft: 5
    property real traceRight: 5
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
        visible: root.halo
        anchors.margins: -2
        radius: root.radius
        color: Qt.alpha(root.ink, .025 * root.amount)
    }
    Rectangle {
        anchors.fill: parent
        radius: root.radius
        topLeftRadius: root.topLeftRadius
        topRightRadius: root.topRightRadius
        bottomLeftRadius: root.bottomLeftRadius
        bottomRightRadius: root.bottomRightRadius
        gradient: Gradient {
            orientation: Gradient.Horizontal
            GradientStop {
                position: 0
                color: Qt.alpha(root.ink, .03 + .09 * root.amount)
            }
            GradientStop {
                position: 1
                color: NacreTokens.gradientColour(Qt.alpha(root.ink, .03 + .09 * root.amount), Qt.alpha(NacreTokens.orientSecondary, .02 + .10 * root.amount))
            }
        }
        border.width: root.outline ? 1 : 0
        border.color: Qt.alpha(root.ink, .06 + .18 * root.amount)
    }
    Rectangle {
        objectName: "hoverRevealTrace"
        x: root.traceLeft + Math.max(0, parent.width - root.traceLeft - root.traceRight) * (1 - root.amount) / 2
        y: parent.height - 3
        width: Math.max(0, parent.width - root.traceLeft - root.traceRight) * root.amount
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
                color: NacreTokens.gradientColour(root.ink, NacreTokens.orientSecondary)
            }
        }
    }
}
