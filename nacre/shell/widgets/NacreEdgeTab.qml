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
    Item {
        id: cue
        objectName: "edgeTabHoverCue"
        x: root.edge === "left" ? 0 : root.width - width
        width: 1
        height: 36
        anchors.verticalCenter: parent.verticalCenter
        Rectangle {
            objectName: "edgeTabHoverFill"
            width: parent.width
            height: parent.height * root.amount
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
            objectName: "edgeTabStroke"
            strokeWidth: 1.25
            strokeColor: NacreTokens.accent
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
