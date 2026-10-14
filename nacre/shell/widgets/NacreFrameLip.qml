import QtQuick
import QtQuick.Shapes
import qs.services

Item {
    id: root
    required property string edge
    property bool pointerActivation: false
    activeFocusOnTab: visible && enabled
    Keys.onReturnPressed: event => {
        pointerActivation = false;
        clicked();
        event.accepted = true;
    }
    Keys.onSpacePressed: event => {
        pointerActivation = false;
        clicked();
        event.accepted = true;
    }
    property bool decorationEnabled: true
    property real base: 10
    property real progress: 0
    property bool active: false
    property alias hovered: pointer.containsMouse
    readonly property real pointerX: pointer.mouseX
    readonly property real pointerY: pointer.mouseY
    readonly property real shoulder: edge === "top" ? 12 : 10
    readonly property rect visualRect: edge === "top" ? Qt.rect((width - Math.min(160, width)) / 2, 0, Math.min(160, width), height) : Qt.rect(0, (height - Math.min(104, height)) / 2, width, Math.min(104, height))
    readonly property rect ridgeRect: edge === "top" ? Qt.rect(visualRect.x + shoulder, 1, Math.max(0, visualRect.width - 2 * shoulder), Math.max(0, height - 1)) : Qt.rect(edge === "left" ? base : 0, visualRect.y + shoulder, Math.max(0, width - base), Math.max(0, visualRect.height - 2 * shoulder))
    readonly property bool sheenRunning: sheen.running
    property bool sweepPending: false
    property real sweep: -.2
    property real accentStrength: hovered || active ? .92 : .72
    signal entered(int buttons)
    signal moved(int buttons)
    signal exited
    signal clicked
    Behavior on accentStrength {
        NumberAnimation {
            id: strengthAnimation
            duration: root.visible && NacreTokens.motionEnabled ? 120 : 0
        }
    }
    function highlight() {
        if (!decorationEnabled || sweepPending || !visible || !NacreTokens.motionEnabled)
            return;
        sweepPending = true;
        Qt.callLater(() => {
            sweepPending = false;
            if (root.visible && NacreTokens.motionEnabled)
                sheen.restart();
        });
    }
    onHoveredChanged: if (hovered)
        highlight()
    onActiveChanged: if (active)
        highlight()
    onVisibleChanged: {
        if (!visible) {
            sheen.stop();
            strengthAnimation.complete();
        } else if (active || hovered) {
            highlight();
        }
    }
    Connections {
        target: NacreTokens
        function onMotionEnabledChanged() {
            if (!NacreTokens.motionEnabled) {
                sheen.stop();
                strengthAnimation.complete();
            }
        }
    }
    NumberAnimation {
        id: sheen
        target: root
        property: "sweep"
        from: -.2
        to: 1.2
        duration: 240
        easing.type: Easing.InOutCubic
    }
    function flatPath() {
        const w = visualRect.width, h = visualRect.height, s = shoulder;
        if (edge === "top") {
            const middle = (1 + h) / 2;
            return `M0 0 H${w} V1 Q${w - s} 1 ${w - s} ${middle} Q${w - s} ${h} ${w - 2 * s} ${h} H${2 * s} Q${s} ${h} ${s} ${middle} Q${s} 1 0 1 Z`;
        }
        const middle = (base + w) / 2;
        const x = value => edge === "left" ? value : w - value;
        return `M${x(0)} 0 H${x(base)} Q${x(base)} ${s} ${x(middle)} ${s} Q${x(w)} ${s} ${x(w)} ${2 * s} V${h - 2 * s} Q${x(w)} ${h - s} ${x(middle)} ${h - s} Q${x(base)} ${h - s} ${x(base)} ${h} H${x(0)} Z`;
    }
    readonly property string outline: flatPath()
    Shape {
        visible: root.decorationEnabled
        x: root.visualRect.x
        y: root.visualRect.y
        width: root.visualRect.width
        height: root.visualRect.height
        preferredRendererType: Shape.CurveRenderer
        ShapePath {
            fillColor: NacreTokens.body
            strokeWidth: 0
            PathSvg {
                path: root.outline
            }
        }
    }
    Item {
        id: enamel
        visible: root.decorationEnabled
        x: root.visualRect.x + (root.edge === "left" ? root.base : 0)
        y: root.visualRect.y + (root.edge === "top" ? 1 : 0)
        width: root.edge === "top" ? root.visualRect.width : Math.max(0, root.width - root.base)
        height: root.edge === "top" ? Math.max(0, root.visualRect.height - 1) : root.visualRect.height
        clip: true
        Shape {
            x: root.edge === "left" ? -root.base : 0
            y: root.edge === "top" ? -1 : 0
            width: root.visualRect.width
            height: root.visualRect.height
            preferredRendererType: Shape.CurveRenderer
            ShapePath {
                strokeWidth: 0
                fillGradient: LinearGradient {
                    x1: 0
                    y1: 0
                    x2: root.edge === "top" ? root.visualRect.width : 0
                    y2: root.edge === "top" ? 0 : root.visualRect.height
                    GradientStop {
                        position: 0
                        color: Qt.tint(NacreTokens.body, Qt.alpha(NacreTokens.accent, root.accentStrength))
                    }
                    GradientStop {
                        position: 1
                        color: NacreTokens.gradientColour(Qt.tint(NacreTokens.body, Qt.alpha(NacreTokens.accent, root.accentStrength)), Qt.tint(NacreTokens.body, Qt.alpha(NacreTokens.orientSecondary, root.accentStrength)))
                    }
                }
                PathSvg {
                    path: root.outline
                }
            }
        }
        Shape {
            x: root.edge === "left" ? -root.base : 0
            y: root.edge === "top" ? -1 : 0
            width: root.visualRect.width
            height: root.visualRect.height
            visible: root.sheenRunning
            preferredRendererType: Shape.CurveRenderer
            ShapePath {
                strokeWidth: 0
                fillGradient: LinearGradient {
                    x1: root.edge === "top" ? (root.sweep - .1) * root.visualRect.width : 0
                    y1: root.edge === "top" ? 0 : (root.sweep - .1) * root.visualRect.height
                    x2: root.edge === "top" ? (root.sweep + .1) * root.visualRect.width : 0
                    y2: root.edge === "top" ? 0 : (root.sweep + .1) * root.visualRect.height
                    GradientStop {
                        position: 0
                        color: "transparent"
                    }
                    GradientStop {
                        position: .5
                        color: Qt.alpha("#fff9f1", .42)
                    }
                    GradientStop {
                        position: 1
                        color: "transparent"
                    }
                }
                PathSvg {
                    path: root.outline
                }
            }
        }
    }
    Rectangle {
        visible: root.decorationEnabled && root.edge === "top" && root.progress > 0
        x: root.visualRect.x + root.shoulder * 2
        y: 2
        width: Math.max(0, root.visualRect.width - root.shoulder * 4) * Math.max(0, Math.min(1, root.progress))
        height: 2
        radius: 1
        color: "#f2efee"
    }
    MouseArea {
        id: pointer
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        acceptedButtons: Qt.LeftButton
        onEntered: root.entered(pressedButtons)
        onPositionChanged: event => root.moved(event.buttons)
        onExited: root.exited()
        onClicked: {
            root.pointerActivation = true;
            root.clicked();
        }
    }
    Accessible.role: Accessible.Button
    Accessible.name: active && edge !== "top" ? edge === "left" ? NacreI18n.tr("ui.frame-lip.name.close-assistant", "Close assistant") : NacreI18n.tr("ui.frame-lip.name.close-control-center", "Close control center") : edge === "top" ? NacreI18n.tr("ui.frame-lip.name.open-dashboard", "Open dashboard") : edge === "left" ? NacreI18n.tr("ui.frame-lip.name.open-assistant", "Open assistant") : NacreI18n.tr("ui.frame-lip.name.open-control-center", "Open control center")
    Accessible.onPressAction: {
        pointerActivation = false;
        clicked();
    }
}
