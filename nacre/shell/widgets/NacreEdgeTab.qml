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
        height: 18
        visible: width >= 4
        x: (parent.width - width) / 2
        anchors.verticalCenter: parent.verticalCenter
        opacity: .75 + .25 * root.amount
        preferredRendererType: Shape.CurveRenderer
        ShapePath {
            strokeWidth: 1.25
            strokeGradient: LinearGradient {
                x1: 0
                y1: 0
                x2: 0
                y2: arrow.height
                GradientStop {
                    position: 0
                    color: NacreTokens.accent
                }
                GradientStop {
                    position: 1
                    color: NacreTokens.orientSecondary
                }
            }
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
}
