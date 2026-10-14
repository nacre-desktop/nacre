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
    Rectangle {
        id: patch
        objectName: "edgeTabPatch"
        width: Math.max(0, parent.width - 2)
        height: 46
        anchors.centerIn: parent
        radius: Math.min(3, width / 2)
        opacity: .72 + .23 * root.amount
        // Foreground contrast uses the painted gradient's midpoint over the wall.
        readonly property color midpoint: Qt.tint(NacreTokens.accent, Qt.alpha(NacreTokens.orientSecondary, .5))
        color: Qt.tint(NacreTokens.frame, Qt.alpha(midpoint, opacity))
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
    Shape {
        id: arrow
        objectName: "edgeTabArrow"
        readonly property bool pointsRight: root.edge === "left" ? !root.active : root.active
        width: Math.max(0, Math.min(8, parent.width - 2))
        height: 18
        visible: width >= 4
        x: (parent.width - width) / 2
        anchors.verticalCenter: parent.verticalCenter
        preferredRendererType: Shape.CurveRenderer
        ShapePath {
            strokeWidth: 1.25
            strokeColor: Qt.tint(NacreTokens.focusInk(patch.color), Qt.alpha(NacreTokens.accent, .2 * root.amount))
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
