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
            readonly property var liveCue: NacrePearls.magneticCue?.screen === modelData.name ? NacrePearls.magneticCue : null
            property var retainedCue: null
            readonly property var cue: liveCue || retainedCue
            onLiveCueChanged: {
                if (liveCue) {
                    retainedCue = liveCue;
                    expiry.stop();
                } else
                    expiry.restart();
            }
            Timer {
                id: expiry
                interval: NacreTokens.motionEnabled ? 190 : 1
                onTriggered: if (!root.liveCue)
                    root.retainedCue = null
            }
            visible: !!cue
            Item {
                anchors.fill: parent
                opacity: root.liveCue ? 1 : 0
                Behavior on opacity {
                    NumberAnimation {
                        duration: NacreTokens.motionEnabled ? 160 : 0
                    }
                }
                NacreDockFilm {
                    anchors.fill: parent
                    origin: Qt.point((root.cue?.origin.x || 0) - root.modelData.x, (root.cue?.origin.y || 0) - root.modelData.y)
                    tip: root.liveCue ? Qt.point(root.liveCue.tip.x - root.modelData.x, root.liveCue.tip.y - root.modelData.y) : origin
                    Behavior on tip {
                        enabled: !root.liveCue && NacreTokens.motionEnabled
                        PropertyAnimation {
                            duration: 160
                            easing.type: Easing.OutCubic
                        }
                    }
                    vertical: ["top", "bottom"].includes(root.cue?.edge)
                    armed: root.cue?.armed === true
                }
            }
        }
    }
}
