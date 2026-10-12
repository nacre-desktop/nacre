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

    implicitHeight: Math.max(34, label.implicitHeight + 12)
    implicitWidth: label.implicitWidth + (compact ? 20 : 28) + (icon ? (compact ? 22 : 25) : 0)
    radius: 17
    color: selected ? NacreColours.palette.m3primary : NacreColours.palette.m3surfaceContainerHigh
    opacity: enabled ? 1 : 0.4

    Row {
        anchors.centerIn: parent
        width: parent.width - (root.compact ? 20 : 28)
        spacing: root.compact ? 5 : 7

        NacreIcon {
            anchors.verticalCenter: parent.verticalCenter
            text: root.icon
            visible: root.icon.length > 0
            font.pointSize: root.compact ? 12 : 14
            color: root.selected ? NacreColours.palette.m3onPrimary : NacreColours.palette.m3onSurface
        }

        NacreText {
            id: label

            text: root.text
            width: parent.width - (root.icon ? (root.compact ? 22 : 25) : 0)
            wrapMode: Text.Wrap
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
