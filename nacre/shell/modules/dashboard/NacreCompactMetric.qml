import QtQuick
import QtQuick.Shapes
import qs.widgets

Item {
    id: root
    property real value: NaN
    property string label: ""
    readonly property bool available: Number.isFinite(value)
    readonly property real fraction: available ? Math.max(0, Math.min(1, value)) : 0
    readonly property real diameter: Math.max(20, Math.min(width - 12, height - 28, 64))
    implicitHeight: 92
    Accessible.role: Accessible.Indicator
    Accessible.name: label + " " + (available ? Math.round(fraction * 100) + "%" : "Unavailable")
    Shape {
        x: (root.width - width) / 2
        width: root.diameter
        height: width
        preferredRendererType: Shape.CurveRenderer
        ShapePath {
            fillColor: "transparent"
            strokeColor: Qt.alpha(NacreTokens.mutedInk, .15)
            strokeWidth: 4
            PathAngleArc {
                centerX: root.diameter / 2
                centerY: root.diameter / 2
                radiusX: root.diameter / 2 - 4
                radiusY: radiusX
                startAngle: -90
                sweepAngle: 360
            }
        }
        ShapePath {
            fillColor: "transparent"
            strokeWidth: 4
            capStyle: ShapePath.RoundCap
            strokeGradient: LinearGradient {
                x1: 0
                y1: root.diameter
                x2: root.diameter
                y2: 0
                GradientStop {
                    position: 0
                    color: NacreTokens.accent
                }
                GradientStop {
                    position: 1
                    color: NacreTokens.orientSecondary
                }
            }
            PathAngleArc {
                centerX: root.diameter / 2
                centerY: root.diameter / 2
                radiusX: root.diameter / 2 - 4
                radiusY: radiusX
                startAngle: -90
                sweepAngle: root.fraction * 360
            }
        }
    }
    NacreText {
        x: 0
        y: (root.diameter - height) / 2
        width: parent.width
        text: root.available ? Math.round(root.fraction * 100) + "%" : "—"
        font.pointSize: 14
        horizontalAlignment: Text.AlignHCenter
        color: NacreTokens.accent
    }
    NacreText {
        width: parent.width
        y: root.diameter + 8
        text: root.label
        font.pointSize: 10
        horizontalAlignment: Text.AlignHCenter
        color: NacreTokens.mutedInk
    }
}
