import QtQuick
import QtQuick.Shapes

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
    Shape {
        id: arrow
        objectName: "edgeTabArrow"
        readonly property bool pointsRight: root.edge === "left" ? !root.active : root.active
        width: Math.max(0, Math.min(8, parent.width - 2))
        height: 22
        visible: width >= 4
        x: (parent.width - width) / 2
        anchors.verticalCenter: parent.verticalCenter
        preferredRendererType: Shape.CurveRenderer
        ShapePath {
            strokeWidth: 2
            strokeColor: Qt.tint(NacreTokens.ink, Qt.alpha(NacreTokens.accent, .25 + .75 * root.amount))
            fillColor: "transparent"
            capStyle: ShapePath.RoundCap
            joinStyle: ShapePath.RoundJoin
            startX: arrow.pointsRight ? 1 : arrow.width - 1
            startY: 1
            PathLine {
                x: arrow.pointsRight ? arrow.width - 1 : 1
                y: arrow.height / 2
            }
            PathLine {
                x: arrow.pointsRight ? 1 : arrow.width - 1
                y: arrow.height - 1
            }
        }
    }
    Rectangle {
        objectName: "edgeTabTrace"
        x: root.edge === "left" ? 0 : parent.width - 2
        anchors.verticalCenter: parent.verticalCenter
        width: 2
        height: Math.min(36, Math.max(0, parent.height - 24)) * root.amount
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
