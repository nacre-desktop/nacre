import QtQuick
import qs.widgets
import qs.services

NacreSurface {
    id: root
    required property var entry
    property bool selected: false
    signal chosen
    function choose() {
        chosen();
    }
    implicitHeight: NacreTokens.space.huge * 3 + NacreTokens.space.lg
    radius: NacreTokens.space.lg
    color: entry.surface ? "#" + entry.surface : NacreTokens.raised
    border.width: selected ? 2 : 1
    border.color: selected ? NacreTokens.focusInk(color) : Qt.alpha(NacreTokens.outline, .45)
    Row {
        x: NacreTokens.space.compact
        y: NacreTokens.space.compact
        width: parent.width - 20
        spacing: NacreTokens.space.xs
        Repeater {
            model: root.entry.swatches || []
            Rectangle {
                required property var modelData
                width: (parent.width - 8) / 3
                height: NacreTokens.space.xxl
                radius: NacreTokens.space.sm
                color: "#" + modelData
            }
        }
    }
    NacreText {
        x: NacreTokens.space.compact
        objectName: "paletteName"
        y: NacreTokens.space.huge + NacreTokens.space.md
        width: parent.width - 20
        height: NacreTokens.space.huge + NacreTokens.space.xs
        text: NacreI18n.option("palette-name", root.entry.name) || NacreI18n.tr("ui.palette-tile.text.color", "Color")
        font.pointSize: NacreTokens.textSize.caption
        wrapMode: Text.Wrap
        maximumLineCount: 2
        elide: Text.ElideRight
        color: NacreTokens.focusInk(root.color)
    }
    Row {
        objectName: "paletteInUse"
        x: NacreTokens.space.compact
        y: NacreTokens.space.huge * 2 + NacreTokens.space.xl
        spacing: NacreTokens.space.xs
        visible: root.selected
        NacreIcon {
            text: "check"
            font.pointSize: NacreTokens.textSize.caption
            color: NacreTokens.focusInk(root.color)
        }
        NacreText {
            text: NacreI18n.tr("ui.palette-tile.text.in-use", "In use")
            font.pointSize: NacreTokens.textSize.body
            color: NacreTokens.focusInk(root.color)
        }
    }
    NacreInteraction {
        accessibleName: root.selected ? NacreI18n.tr("palette.tile.selected", "{name}, in use", {
            name: root.entry.name ? NacreI18n.option("palette-name", root.entry.name) : NacreI18n.tr("ui.palette-tile.accessibleName.color-palette", "Color palette")
        }) : root.entry.name ? NacreI18n.option("palette-name", root.entry.name) : NacreI18n.tr("ui.palette-tile.accessibleName.color-palette", "Color palette")
        function onClicked() {
            root.choose();
        }
    }
}
