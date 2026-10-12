import QtQuick
import Quickshell
import Quickshell.Wayland
import qs.widgets
import qs.services
import "geometry.js" as Geometry

Scope {
    Variants {
        model: Quickshell.screens
        PanelWindow {
            id: root
            required property var modelData
            screen: modelData
            anchors {
                top: true
                bottom: true
                left: true
                right: true
            }
            color: "transparent"
            exclusionMode: ExclusionMode.Ignore
            WlrLayershell.namespace: "nacre-pearl-drag"
            WlrLayershell.layer: WlrLayer.Overlay
            WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
            mask: Region {}
            visible: !!NacrePearls.drag
            readonly property var gesture: NacrePearls.drag
            readonly property var destination: gesture?.entry ? Geometry.docking(gesture.kind, gesture.point, NacrePearls.monitors()) : null
            readonly property bool targeted: destination?.screen === modelData.name
            readonly property bool containsPointer: gesture && gesture.point.x >= modelData.x && gesture.point.y >= modelData.y && gesture.point.x <= modelData.x + modelData.width && gesture.point.y <= modelData.y + modelData.height
            Canvas {
                id: tether
                anchors.fill: parent
                visible: !!root.gesture && !root.gesture.released && !root.gesture.entry && NacreTokens.motionEnabled
                renderTarget: Canvas.FramebufferObject
                onVisibleChanged: requestPaint()
                Connections {
                    target: NacrePearls
                    function onDragChanged() {
                        tether.requestPaint();
                    }
                }
                onPaint: {
                    const context = getContext("2d");
                    context.clearRect(0, 0, width, height);
                    if (!visible || !root.gesture)
                        return;
                    const start = root.gesture.start, end = root.gesture.point;
                    const ax = start.x - root.modelData.x, ay = start.y - root.modelData.y;
                    const bx = end.x - root.modelData.x, by = end.y - root.modelData.y;
                    const gradient = context.createLinearGradient(ax, ay, bx, by);
                    gradient.addColorStop(0, Qt.alpha(NacreTokens.accent, .45));
                    gradient.addColorStop(1, Qt.alpha(NacreTokens.orientSecondary, .65));
                    context.strokeStyle = gradient;
                    context.lineWidth = 2;
                    context.beginPath();
                    context.moveTo(ax, ay);
                    context.bezierCurveTo(ax, ay + (by - ay) * .65, bx, ay + (by - ay) * .35, bx, by);
                    context.stroke();
                }
            }
            Rectangle {
                visible: root.targeted
                color: Qt.alpha(NacreTokens.accent, .35)
                radius: 6
                x: root.destination?.edge === "right" ? root.width - 12 : root.destination?.edge === "left" ? 4 : root.width / 2 - 110
                y: root.destination?.edge === "bottom" ? root.height - 12 : root.destination?.edge === "top" ? 4 : root.height / 2 - 110
                width: ["top", "bottom"].includes(root.destination?.edge) ? 220 : 8
                height: ["top", "bottom"].includes(root.destination?.edge) ? 8 : 220
            }
            Item {
                id: ghost
                visible: root.containsPointer && !!root.gesture?.url
                width: Math.min(420, root.gesture?.width || 420)
                height: width * (root.gesture?.height || 300) / (root.gesture?.width || 420)
                x: (root.gesture?.point.x || 0) - root.modelData.x - width / 2
                y: (root.gesture?.point.y || 0) - root.modelData.y + 20
                opacity: root.gesture?.released ? 0 : root.gesture?.ready ? .9 : .5
                scale: root.gesture?.ready ? 1 : .94
                Behavior on opacity {
                    NumberAnimation {
                        duration: NacreTokens.motionEnabled ? 180 : 0
                    }
                }
                Behavior on scale {
                    NumberAnimation {
                        duration: NacreTokens.motionEnabled ? 150 : 0
                        easing.type: Easing.OutCubic
                    }
                }
                NacreSurface {
                    anchors.fill: parent
                    radius: 18
                    color: NacreTokens.body
                    border.color: NacreTokens.accent
                    border.width: 1
                }
                Image {
                    anchors.fill: parent
                    anchors.margins: 4
                    cache: false
                    source: root.gesture?.url || ""
                    fillMode: Image.PreserveAspectFit
                }
                NacreSurface {
                    anchors.horizontalCenter: parent.horizontalCenter
                    y: parent.height + 8
                    width: label.implicitWidth + 24
                    height: 30
                    radius: 15
                    color: NacreTokens.body
                    NacreText {
                        id: label
                        anchors.centerIn: parent
                        text: root.targeted ? "Release to dock" : root.gesture?.ready ? "Release to open in window" : "Pull away from the edge"
                        font.pointSize: 10
                    }
                }
            }
        }
    }
}
