pragma ComponentBehavior: Bound
import QtQuick
import qs.widgets

NacreSurface {
    id: root
    required property var registry
    required property var store
    readonly property var mode: store.value(registry.schema.settings.find(item => item.id === "colors.palette.mode"))
    readonly property bool light: mode === "light"
    readonly property color paper: light ? "#edf1f4" : "#19232c"
    readonly property color ink: light ? "#202d36" : "#e3ebf1"
    readonly property color accent: store.value(registry.schema.settings.find(item => item.id === "colors.palette.source")) === "wallpaper" ? "#8eb8c9" : "#8eb6eb"
    color: paper
    radius: NacreTokens.radii.small
    implicitHeight: 172
    Row {
        x: NacreTokens.space.xl
        y: NacreTokens.space.xl
        spacing: NacreTokens.space.sm
        Repeater {
            model: [root.accent, "#a6b9b5", "#c8b8c7", root.ink]
            NacreSurface {
                required property color modelData
                width: 28
                height: 28
                radius: NacreTokens.radii.full
                color: modelData
            }
        }
    }
    NacreText {
        x: NacreTokens.space.xl
        y: 62
        text: root.registry.text("settings.registry.ui.preview-label")
        color: root.ink
        font.pointSize: NacreTokens.textSize.title
    }
    NacreSurface {
        x: NacreTokens.space.xl
        y: 102
        width: Math.min(220, root.width - 2 * x)
        height: 46
        radius: NacreTokens.radii.small
        color: root.accent
        NacreText {
            anchors.centerIn: parent
            text: root.registry.text("settings.registry.ui.credit")
            color: NacreTokens.focusInk(parent.color)
            font.pointSize: NacreTokens.textSize.caption
        }
    }
}
