import QtQuick
import QtQuick.Shapes
import qs.services

// Shared nacre membrane for pulling a panel free and drawing it back to the rim.
Item {
    id: root
    property point origin: Qt.point(0, 0)
    property point tip: Qt.point(0, 0)
    property bool vertical: true
    property bool armed: false
    readonly property real distance: Math.abs(vertical ? tip.y - origin.y : tip.x - origin.x)
    property real stretch: Math.min(1, distance / 170)
    opacity: armed ? .88 : .6
    visible: distance > 1 && stretch < .995
    Behavior on stretch {
        enabled: root.visible && NacreTokens.motionEnabled
        NumberAnimation {
            duration: 120
            easing.type: Easing.OutCubic
        }
    }
    Behavior on opacity {
        enabled: NacreTokens.motionEnabled
        NumberAnimation {
            duration: 100
        }
    }
    function filmPath() {
        const startAlong = vertical ? origin.x : origin.y;
        const startAcross = vertical ? origin.y : origin.x;
        const endAlong = vertical ? tip.x : tip.y;
        const endAcross = vertical ? tip.y : tip.x;
        const span = vertical ? 68 : 52;
        const neck = Math.max(3, span * (1 - stretch));
        const delta = endAcross - startAcross;
        function point(along, across) {
            return vertical ? along + " " + across : across + " " + along;
        }
        return "M" + point(startAlong - span, startAcross) + " C" + point(startAlong - span, startAcross + delta * .7) + " " + point(endAlong - neck, endAcross - delta * .3) + " " + point(endAlong - neck, endAcross) + " L" + point(endAlong + neck, endAcross) + " C" + point(endAlong + neck, endAcross - delta * .3) + " " + point(startAlong + span, startAcross + delta * .7) + " " + point(startAlong + span, startAcross) + " Z";
    }
    Shape {
        anchors.fill: parent
        preferredRendererType: Shape.CurveRenderer
        ShapePath {
            strokeWidth: 1
            strokeColor: Qt.alpha(NacreTokens.orientHighlight, .35)
            fillGradient: LinearGradient {
                x1: root.origin.x
                y1: root.origin.y
                x2: root.tip.x
                y2: root.tip.y
                GradientStop {
                    position: 0
                    color: Qt.alpha(NacreTokens.accent, .6)
                }
                GradientStop {
                    position: .55
                    color: Qt.alpha(NacreTokens.orientSecondary, .65)
                }
                GradientStop {
                    position: 1
                    color: Qt.alpha(NacreTokens.orientHighlight, .55)
                }
            }
            PathSvg {
                path: root.filmPath()
            }
        }
    }
}
