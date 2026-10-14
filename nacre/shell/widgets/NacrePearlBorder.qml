import QtQuick
import QtQuick.Shapes

// Painted by the client, so placement cannot reveal an empty compositor outline.
Shape {
    id: root
    property bool engaged: false
    property real strength: engaged ? 1 : 0
    preferredRendererType: Shape.CurveRenderer
    Behavior on strength {
        NumberAnimation {
            duration: NacreTokens.motionEnabled ? 120 : 0
        }
    }
    function blend(accent) {
        return Qt.tint(NacreTokens.outline, Qt.alpha(accent, strength));
    }
    ShapePath {
        fillColor: "transparent"
        strokeWidth: 2
        strokeGradient: LinearGradient {
            x1: 0
            y1: 0
            x2: root.width
            y2: root.height
            GradientStop {
                position: 0
                color: root.blend(NacreTokens.accent)
            }
            GradientStop {
                position: 1
                color: NacreTokens.gradientColour(root.blend(NacreTokens.accent), root.blend(NacreTokens.orientSecondary))
            }
        }
        PathSvg {
            path: {
                const w = Math.max(2, root.width - 1), h = Math.max(2, root.height - 1);
                const r = Math.min(17, (w - 1) / 2, (h - 1) / 2);
                return "M " + (1 + r) + " 1 H " + (w - r) + " A " + r + " " + r + " 0 0 1 " + w + " " + (1 + r) + " V " + (h - r) + " A " + r + " " + r + " 0 0 1 " + (w - r) + " " + h + " H " + (1 + r) + " A " + r + " " + r + " 0 0 1 1 " + (h - r) + " V " + (1 + r) + " A " + r + " " + r + " 0 0 1 " + (1 + r) + " 1 Z";
            }
        }
    }
}
