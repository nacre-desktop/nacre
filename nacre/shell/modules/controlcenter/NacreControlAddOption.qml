import QtQuick
import QtQuick.Layouts
import qs.widgets

Loader {
    id: root
    required property var entry
    property string detail: ""
    property bool settingsRows: false
    signal clicked
    function focusControl() {
        const first = parent => {
            if (!parent || !parent.visible || !parent.enabled)
                return null;
            if (parent.activeFocusOnTab)
                return parent;
            for (const child of parent.children || []) {
                const found = first(child);
                if (found)
                    return found;
            }
            return null;
        };
        first(item)?.forceActiveFocus(Qt.OtherFocusReason);
    }
    sourceComponent: settingsRows ? row : tile
    Component {
        id: tile
        NacreControlTile {
            objectName: root.objectName + "Tile"
            label: root.entry.name
            icon: root.entry.icon
            detail: root.detail
            opacity: enabled ? 1 : .45
            onClicked: root.clicked()
        }
    }
    Component {
        id: row
        Item {
            objectName: root.objectName + "Row"
            implicitWidth: NacreTokens.space.huge * 9
            implicitHeight: Math.max(NacreTokens.space.huge + 2 * NacreTokens.space.sm, content.implicitHeight + 2 * NacreTokens.space.sm)
            opacity: enabled ? 1 : .45
            RowLayout {
                id: content
                anchors.fill: parent
                anchors.topMargin: NacreTokens.space.sm
                anchors.bottomMargin: NacreTokens.space.sm
                spacing: NacreTokens.space.sm
                NacreIcon {
                    text: root.entry.icon
                    font.pointSize: NacreTokens.iconSize.medium
                    color: NacreTokens.accent
                }
                NacreText {
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    text: root.entry.name
                    font.pointSize: NacreTokens.textSize.body
                    wrapMode: Text.Wrap
                }
                NacreControlButton {
                    objectName: root.objectName + "Button"
                    icon: "add"
                    label: root.entry.name + ": " + root.detail
                    onClicked: root.clicked()
                }
            }
        }
    }
}
