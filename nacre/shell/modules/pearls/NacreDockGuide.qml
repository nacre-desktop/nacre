import QtQuick
import Quickshell
import Quickshell.Wayland
import qs.widgets
import qs.services

Scope {
    Variants {
        model: Quickshell.screens
        PanelWindow {
            id: root
            required property var modelData
            screen: modelData
            anchors {
                left: true
                right: true
                top: true
                bottom: true
            }
            color: "transparent"
            exclusionMode: ExclusionMode.Ignore
            WlrLayershell.namespace: "nacre-dock-guide"
            WlrLayershell.layer: WlrLayer.Overlay
            WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
            mask: Region {}
            readonly property var destination: NacrePearls.nativeDrag?.destination
            visible: destination?.screen === modelData.name
            Rectangle {
                readonly property bool vertical: ["left", "right"].includes(root.destination?.edge)
                width: vertical ? 3 : 220
                height: vertical ? 180 : 3
                x: vertical ? (root.destination?.edge === "left" ? 4 : root.width - 7) : (root.width - width) / 2
                y: vertical ? (root.height - height) / 2 : root.destination?.edge === "top" ? 4 : root.height - 7
                radius: 1.5
                gradient: Gradient {
                    orientation: Gradient.Horizontal
                    GradientStop {
                        position: 0
                        color: NacreTokens.accent
                    }
                    GradientStop {
                        position: 1
                        color: NacreTokens.orientSecondary
                    }
                }
            }
            NacreSurface {
                width: label.implicitWidth + 28
                height: 34
                radius: 10
                color: NacreTokens.body
                x: root.destination?.edge === "left" ? 20 : root.destination?.edge === "right" ? root.width - width - 20 : (root.width - width) / 2
                y: root.destination?.edge === "top" ? 18 : root.destination?.edge === "bottom" ? root.height - height - 18 : (root.height - height) / 2
                NacreText {
                    id: label
                    anchors.centerIn: parent
                    text: "Release to dock"
                    font.pointSize: 11
                }
            }
        }
    }
}
