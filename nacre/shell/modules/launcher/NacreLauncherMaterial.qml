import QtQuick
import QtQuick.Shapes

Item {
    id: root

    property real cornerRadius: 24
    property color fillColor: "#202124"
    property color highlightColor: "white"
    property color accentColor: "#9aa0a6"
    property color secondaryColor: "#5f6368"
    property real sheenOpacity: .16
    property point gradientStart: Qt.point(0, 0)
    property point gradientEnd: Qt.point(width, height)

    readonly property real radius: Math.max(0, Math.min(cornerRadius, height / 2, width / 4))
    readonly property real bodyLeft: radius
    readonly property real bodyRight: width - radius

    Shape {
        anchors.fill: parent
        preferredRendererType: Shape.CurveRenderer

        ShapePath {
            fillColor: root.fillColor
            strokeColor: "transparent"
            strokeWidth: 0
            startX: root.bodyLeft + root.radius
            startY: 0
            PathLine {
                x: root.bodyRight - root.radius
                y: 0
            }
            PathQuad {
                x: root.bodyRight
                y: root.radius
                controlX: root.bodyRight
                controlY: 0
            }
            PathLine {
                x: root.bodyRight
                y: root.height - root.radius
            }
            PathQuad {
                x: root.width
                y: root.height
                controlX: root.bodyRight
                controlY: root.height
            }
            PathLine {
                x: 0
                y: root.height
            }
            PathQuad {
                x: root.bodyLeft
                y: root.height - root.radius
                controlX: root.bodyLeft
                controlY: root.height
            }
            PathLine {
                x: root.bodyLeft
                y: root.radius
            }
            PathQuad {
                x: root.bodyLeft + root.radius
                y: 0
                controlX: root.bodyLeft
                controlY: 0
            }
            PathLine {
                x: root.bodyLeft + root.radius
                y: 0
            }
        }
    }

    Shape {
        anchors.fill: parent
        preferredRendererType: Shape.CurveRenderer
        opacity: root.sheenOpacity

        ShapePath {
            fillColor: "transparent"
            strokeWidth: 1
            capStyle: ShapePath.FlatCap
            strokeGradient: LinearGradient {
                x1: root.gradientStart.x
                y1: root.gradientStart.y
                x2: root.gradientEnd.x
                y2: root.gradientEnd.y
                GradientStop {
                    position: 0
                    color: root.highlightColor
                }
                GradientStop {
                    position: .38
                    color: root.accentColor
                }
                GradientStop {
                    position: 1
                    color: root.secondaryColor
                }
            }
            startX: 0
            startY: root.height
            PathQuad {
                x: root.bodyLeft
                y: root.height - root.radius
                controlX: root.bodyLeft
                controlY: root.height
            }
            PathLine {
                x: root.bodyLeft
                y: root.radius
            }
            PathQuad {
                x: root.bodyLeft + root.radius
                y: 0
                controlX: root.bodyLeft
                controlY: 0
            }
            PathLine {
                x: root.bodyRight - root.radius
                y: 0
            }
            PathQuad {
                x: root.bodyRight
                y: root.radius
                controlX: root.bodyRight
                controlY: 0
            }
            PathLine {
                x: root.bodyRight
                y: root.height - root.radius
            }
            PathQuad {
                x: root.width
                y: root.height
                controlX: root.bodyRight
                controlY: root.height
            }
        }
    }
}
