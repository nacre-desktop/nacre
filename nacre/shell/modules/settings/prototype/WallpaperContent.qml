import QtQuick
import qs.widgets
import qs.services

Column {
    required property var registry
    spacing: NacreTokens.space.md
    NacreClip {
        width: parent.width
        height: NacreTokens.space.huge * 5
        radius: NacreTokens.radii.medium
        color: NacreTokens.body
        Image {
            anchors.fill: parent
            source: NacreWallpapers.thumbnail ? "file://" + NacreWallpapers.thumbnail : ""
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
        }
    }
    Row {
        anchors.right: parent.right
        spacing: NacreTokens.space.sm
        ActionButton {
            text: registry.text("settings.registry.wallpaper.label")
            icon: "wallpaper"
            onClicked: NacrePanelState.openMode("wallpaper")
        }
    }
}
