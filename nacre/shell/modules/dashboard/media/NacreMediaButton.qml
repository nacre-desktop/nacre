import QtQuick
import qs.widgets

NacreSurface {
    id: root
    property string icon: ""
    property string label: ""
    property bool available: true
    property bool selected: false
    property color accent: NacreTokens.accent
    property color ink: NacreTokens.ink
    signal clicked
    implicitWidth: 42
    implicitHeight: 42
    radius: width / 2
    color: selected ? root.accent : "transparent"
    opacity: available ? 1 : .35
    NacreIcon {
        anchors.centerIn: parent
        text: root.icon
        font.pointSize: 22
        color: root.selected ? NacreTokens.focusInk(root.color) : root.ink
    }
    NacreInteraction {
        disabled: !root.available
        accessibleName: root.label
        function onClicked() {
            root.clicked();
        }
    }
}
