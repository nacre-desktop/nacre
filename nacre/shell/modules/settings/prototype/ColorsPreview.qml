import QtQuick
import qs.widgets
import qs.services

NacreClip {
    id: root
    required property var registry
    required property var store
    color: NacreTokens.frame
    radius: NacreTokens.radii.medium
    implicitHeight: NacreTokens.space.huge * 5
    Image {
        anchors.fill: parent
        source: NacreWallpapers.thumbnail ? "file://" + NacreWallpapers.thumbnail : ""
        fillMode: Image.PreserveAspectCrop
        asynchronous: true
    }
    NacreSurface {
        x: NacreTokens.space.sm
        y: NacreTokens.space.sm
        width: parent.width - x * 2
        height: NacreTokens.space.xxl
        color: NacreTokens.body
        radius: NacreTokens.radii.small
        NacreIcon {
            x: NacreTokens.space.sm
            anchors.verticalCenter: parent.verticalCenter
            text: "apps"
            font.pointSize: NacreTokens.iconSize.small
        }
        NacreText {
            anchors.centerIn: parent
            text: registry.text("settings.registry.ui.preview-label")
            font.pointSize: NacreTokens.textSize.caption
        }
    }
    NacreSurface {
        x: NacreTokens.space.sm
        y: NacreTokens.space.huge + NacreTokens.space.sm
        width: Math.min(parent.width - x * 2, NacreTokens.space.huge * 8)
        height: NacreTokens.space.huge * 3
        radius: NacreTokens.radii.medium
        color: NacreTokens.raised
        NacreText {
            x: NacreTokens.space.md
            y: NacreTokens.space.md
            width: parent.width - x * 2
            text: registry.text("settings.registry.panels.label")
            font.pointSize: NacreTokens.textSize.caption
        }
        NacreSurface {
            x: NacreTokens.space.md
            y: NacreTokens.space.huge + NacreTokens.space.sm
            width: parent.width - x * 2
            height: NacreTokens.space.xxl + NacreTokens.space.sm
            radius: NacreTokens.radii.small
            color: NacreTokens.accent
            NacreText {
                anchors.centerIn: parent
                text: registry.text("settings.registry.ui.primary-action")
                color: NacreTokens.focusInk(parent.color)
                font.pointSize: NacreTokens.textSize.caption
            }
        }
    }
}
