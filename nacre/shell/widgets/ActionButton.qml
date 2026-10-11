import QtQuick
import qs.services

NacreSurface {
    id: root

    property string text
    property string icon: ""
    property string accessibleLabel: ""
    property bool selected: false
    property bool compact: false

    signal clicked

    implicitHeight: 34
    implicitWidth: label.implicitWidth + (compact ? 20 : 28) + (icon ? (compact ? 22 : 25) : 0)
    radius: 17
    color: selected ? NacreColours.palette.m3primary : NacreColours.palette.m3surfaceContainerHigh
    opacity: enabled ? 1 : 0.4

    Row {
        anchors.centerIn: parent
        spacing: root.compact ? 5 : 7

        NacreIcon {
            text: root.icon
            visible: root.icon.length > 0
            font.pointSize: root.compact ? 12 : 14
            color: root.selected ? NacreColours.palette.m3onPrimary : NacreColours.palette.m3onSurface
        }

        NacreText {
            id: label

            text: root.text
            font.pointSize: root.compact ? 10 : 12
            color: root.selected ? NacreColours.palette.m3onPrimary : NacreColours.palette.m3onSurface
        }
    }

    NacreInteraction {
        accessibleName: root.accessibleLabel || root.text || root.icon
        function onClicked() {
            root.clicked();
        }
    }
}
