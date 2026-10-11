import QtQuick
import QtQuick.Shapes
import qs.widgets

Item {
    id: root
    property real value: NaN
    property string label: ""
    property string detail: ""
    property color first: NacreTokens.accent
    property color second: NacreTokens.orientSecondary
    readonly property bool available: Number.isFinite(value)
    implicitWidth: 180
    implicitHeight: 185
    Accessible.role: Accessible.Indicator
    Accessible.name: label + " " + (available ? Math.round(value * 100) + "% " : "Unavailable ") + detail
    Item {
        width: root.width
        height: 140
        Repeater {
            model: 12
            Shape {
                id: chamber
                required property int index
                anchors.fill: parent
                preferredRendererType: Shape.CurveRenderer
                ShapePath {
                    fillColor: "transparent"
                    strokeWidth: 3 + chamber.index * .4
                    capStyle: ShapePath.RoundCap
                    strokeColor: !root.available || chamber.index >= Math.ceil(root.value * 12) ? Qt.alpha(NacreTokens.mutedInk, .16) : Qt.tint(root.first, Qt.alpha(root.second, chamber.index / 11))
                    PathAngleArc {
                        centerX: root.width / 2
                        centerY: 92
                        radiusX: Math.min(root.width, 190) / 2 - 16
                        radiusY: 70
                        startAngle: 145 + chamber.index * 20
                        sweepAngle: 13
                    }
                }
            }
        }
    }
    Column {
        y: 65
        anchors.horizontalCenter: parent.horizontalCenter
        width: parent.width
        spacing: 7
        NacreText {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            text: root.available ? Math.round(root.value * 100) + "%" : "—"
            font.pointSize: 25
            color: root.first
        }
        NacreText {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            text: root.label
            font.pointSize: 11
        }
        NacreText {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            text: root.detail
            color: NacreTokens.mutedInk
            font.pointSize: 9
            elide: Text.ElideRight
        }
    }
}
