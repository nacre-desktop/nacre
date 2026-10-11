import QtQuick
import qs.widgets

NacreSurface {
    id: root
    required property var entry
    property bool selected: false
    signal chosen
    function choose() {
        chosen();
    }
    implicitHeight: 110
    radius: 14
    color: entry.surface ? "#" + entry.surface : NacreTokens.raised
    border.width: selected ? 2 : 1
    border.color: selected ? NacreTokens.focusInk(color) : Qt.alpha(NacreTokens.outline, .45)
    Row {
        x: 10
        y: 10
        width: parent.width - 20
        spacing: 4
        Repeater {
            model: root.entry.swatches || []
            Rectangle {
                required property var modelData
                width: (parent.width - 8) / 3
                height: 24
                radius: 7
                color: "#" + modelData
            }
        }
    }
    NacreText {
        x: 10
        objectName: "paletteName"
        y: 44
        width: parent.width - 20
        height: 36
        text: root.entry.name || "Color"
        font.pointSize: 10
        wrapMode: Text.Wrap
        maximumLineCount: 2
        elide: Text.ElideRight
        color: NacreTokens.focusInk(root.color)
    }
    Row {
        objectName: "paletteInUse"
        x: 10
        y: 82
        spacing: 5
        visible: root.selected
        NacreIcon {
            text: "check"
            font.pointSize: 10
            color: NacreTokens.focusInk(root.color)
        }
        NacreText {
            text: "In use"
            font.pointSize: 9
            color: NacreTokens.focusInk(root.color)
        }
    }
    NacreInteraction {
        accessibleName: (root.entry.name || "Color palette") + (root.selected ? ", in use" : "")
        function onClicked() {
            root.choose();
        }
    }
}
