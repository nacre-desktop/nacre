import QtQuick
import QtQuick.Shapes
import qs.widgets
import qs.services

// A short-lived film joins the moving panel to the shell until it peels free.
// It is decoration only; the original live panel owns the pointer grab.
Item {
    id: root
    required property var host
    property var releasedFilm: null
    readonly property var liveGesture: NacrePearls.drag?.screen === host.screen.name ? NacrePearls.drag : null
    readonly property var gesture: liveGesture || releasedFilm
    property real breakOpacity: 1
    readonly property real distance: gesture ? Math.hypot(gesture.dx, gesture.dy) : 0
    visible: !!gesture && distance > 1 && stretch < .995 && NacreTokens.motionEnabled
    property real stretch: Math.min(1, distance / 170)
    Behavior on stretch {
        enabled: !!root.gesture && NacreTokens.motionEnabled
        NumberAnimation {
            id: stretching
            duration: 120
            easing.type: Easing.OutCubic
        }
    }
    opacity: breakOpacity * Math.max(0, 1 - stretch * stretch)
    Connections {
        target: NacrePearls
        function onDragChanged() {
            if (root.liveGesture?.released && !breakMotion.running) {
                root.releasedFilm = root.liveGesture;
                breakMotion.start();
                filmExpiry.restart();
            } else if (!root.gesture) {
                breakMotion.stop();
                stretching.complete();
                root.breakOpacity = 1;
            }
        }
    }
    Timer {
        id: filmExpiry
        interval: 190
        onTriggered: {
            root.releasedFilm = null;
            root.breakOpacity = 1;
        }
    }
    NumberAnimation {
        id: breakMotion
        target: root
        property: "breakOpacity"
        to: 0
        duration: 180
        easing.type: Easing.OutCubic
    }
    readonly property bool vertical: ["top", "bottom"].includes(NacrePearls.edge(gesture?.kind))
    readonly property point origin: {
        if (!gesture)
            return Qt.point(0, 0);
        const rect = gesture.body, side = NacrePearls.edge(gesture.kind);
        return Qt.point(rect.x - host.screen.x - host.x + (side === "right" ? rect.width : vertical ? rect.width / 2 : 0), rect.y - host.screen.y - host.y + (side === "bottom" ? rect.height : vertical ? 0 : rect.height / 2));
    }
    readonly property point end: Qt.point(origin.x + (gesture?.dx || 0), origin.y + (gesture?.dy || 0))
    function filmPath() {
        if (!visible)
            return "";
        const startAlong = vertical ? origin.x : origin.y;
        const startAcross = vertical ? origin.y : origin.x;
        const endAlong = vertical ? end.x : end.y;
        const endAcross = vertical ? end.y : end.x;
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
                x2: root.end.x
                y2: root.end.y
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
