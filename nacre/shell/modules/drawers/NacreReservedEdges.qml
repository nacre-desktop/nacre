import QtQuick
import Quickshell
import Quickshell.Wayland
import qs.config
import qs.widgets

Scope {
    id: root
    required property ShellScreen screen
    Variants {
        model: ["top", "left", "right", "bottom"]
        NacreWindow {
            required property string modelData
            readonly property int extent: ({
                    top: NacreFrame.headerHeight,
                    left: NacreFrame.left,
                    right: NacreFrame.right,
                    bottom: NacreFrame.bottom
                })[modelData]
            readonly property bool horizontal: modelData === "top" || modelData === "bottom"
            name: "border-exclusion"
            screen: root.screen
            visible: extent > 0
            implicitWidth: horizontal ? 1 : extent
            implicitHeight: horizontal ? extent : 1
            anchors.top: modelData !== "bottom"
            anchors.bottom: modelData !== "top"
            anchors.left: modelData !== "right"
            anchors.right: modelData !== "left"
            exclusionMode: ExclusionMode.Normal
            exclusiveZone: extent
            mask: Region {}
        }
    }
}
