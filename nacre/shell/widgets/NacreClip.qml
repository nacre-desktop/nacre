import QtQuick
import Quickshell.Widgets

ClippingRectangle {
    id: root

    property bool animateColor: true
    property int transitionDuration: NacreTokens.colorDuration
    property bool ready: false

    color: "transparent"
    radius: 0
    Component.onCompleted: ready = true
    onVisibleChanged: if (!visible && colorTween)
        colorTween.complete()
    onAnimateColorChanged: if (!animateColor && colorTween)
        colorTween.complete()

    Behavior on color {
        id: colorBehavior
        enabled: root.ready && root.visible && root.animateColor
        ColorAnimation {
            id: colorTween
            duration: NacreTokens.colorMotionAllowed ? Math.max(0, root.transitionDuration) : 0
            easing.type: Easing.OutCubic
        }
    }
    Connections {
        target: NacreTokens
        function onColorMotionAllowedChanged() {
            if (!NacreTokens.colorMotionAllowed)
                Qt.callLater(() => {
                    if (colorTween)
                        colorTween.complete();
                });
        }
    }
}
