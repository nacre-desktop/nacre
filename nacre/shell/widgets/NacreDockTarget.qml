import QtQuick
import qs.services

// A quiet landing field, shared by layer pulls and compositor-owned moves.
Item {
    id: root
    property point origin: Qt.point(0, 0)
    property string edge: "top"
    readonly property bool horizontal: edge === "top" || edge === "bottom"
    width: 220
    height: 18
    x: origin.x - width / 2
    y: origin.y - height / 2
    rotation: horizontal ? 0 : 90
    Repeater {
        model: [
            {
                width: 220,
                height: 18,
                opacity: .08
            },
            {
                width: 200,
                height: 8,
                opacity: .18
            },
            {
                width: 180,
                height: 2,
                opacity: .85
            }
        ]
        Rectangle {
            required property var modelData
            anchors.centerIn: parent
            width: modelData.width
            height: modelData.height
            radius: height / 2
            opacity: modelData.opacity
            gradient: Gradient {
                orientation: Gradient.Horizontal
                GradientStop {
                    position: 0
                    color: Qt.alpha(NacreTokens.accent, 0)
                }
                GradientStop {
                    position: .2
                    color: NacreTokens.gradientColour(Qt.alpha(NacreTokens.accent, 0), NacreTokens.accent)
                }
                GradientStop {
                    position: .6
                    color: NacreTokens.gradientColour(Qt.alpha(NacreTokens.accent, 0), NacreTokens.orientSecondary)
                }
                GradientStop {
                    position: .85
                    color: NacreTokens.gradientColour(Qt.alpha(NacreTokens.accent, 0), NacreTokens.orientHighlight)
                }
                GradientStop {
                    position: 1
                    color: NacreTokens.gradientColour(Qt.alpha(NacreTokens.accent, 0), Qt.alpha(NacreTokens.orientHighlight, 0))
                }
            }
        }
    }
}
