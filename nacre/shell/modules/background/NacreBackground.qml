import QtQuick
import Quickshell
import Quickshell.Wayland
import qs.widgets
import qs.services

Variants {
    model: Quickshell.screens
    NacreWindow {
        id: surface
        required property var modelData
        screen: modelData
        name: "background"
        visible: !NacrePanelState.hidden
        anchors.top: true
        anchors.bottom: true
        anchors.left: true
        anchors.right: true
        exclusiveZone: 0
        WlrLayershell.layer: WlrLayer.Background
        WlrLayershell.exclusionMode: ExclusionMode.Ignore
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
        mask: Region {}
        NacreWallpaperScene {
            anchors.fill: parent
            screenName: surface.screen.name
        }
    }
}
