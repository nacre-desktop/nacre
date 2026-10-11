import QtQuick
import qs.services
import qs.widgets

NacreSurface {
    id: root

    property string text: ""
    property string icon: ""
    property bool selected: false
    property bool compact: false
    property bool toggle: false
    property string description: ""
    property string stateText: ""

    signal clicked

    implicitHeight: toggle && description ? 50 : compact ? 32 : 38
    implicitWidth: !text && icon ? implicitHeight : label.implicitWidth + 24 + (icon ? 25 : 0)
    radius: 9
    color: toggle && selected ? Qt.tint(NacreColours.palette.m3surfaceContainerHigh, Qt.alpha(NacreColours.palette.m3primary, .12)) : selected ? NacreColours.palette.m3primary : NacreColours.palette.m3surfaceContainerHigh
    border.width: toggle && selected ? 1 : 0
    border.color: Qt.alpha(NacreColours.palette.m3primary, .45)
    opacity: enabled ? 1 : 0.5

    NacreIcon {
        id: glyph

        x: root.text ? 11 : (parent.width - width) / 2
        anchors.verticalCenter: parent.verticalCenter
        anchors.verticalCenterOffset: root.toggle && root.description ? -9 : 0
        text: root.icon
        visible: !!root.icon
        font.pointSize: root.compact ? 12 : 14
        color: root.selected && !root.toggle ? NacreColours.palette.m3onPrimary : NacreColours.palette.m3onSurface
    }

    NacreText {
        id: label

        x: root.icon ? 36 : 12
        width: Math.max(0, parent.width - x - (root.toggle ? 72 : 12))
        anchors.verticalCenter: parent.verticalCenter
        anchors.verticalCenterOffset: root.toggle && root.description ? -9 : 0
        text: root.text
        textFormat: Text.PlainText
        font.pointSize: root.compact ? 10 : 11
        elide: Text.ElideRight
        color: root.selected && !root.toggle ? NacreColours.palette.m3onPrimary : NacreColours.palette.m3onSurface
    }

    NacreText {
        x: label.x
        y: parent.height / 2 + 4
        width: parent.width - x - 12
        visible: root.toggle && !!root.description
        text: root.description
        font.pointSize: 8
        textFormat: Text.PlainText
        elide: Text.ElideRight
        color: NacreColours.palette.m3onSurfaceVariant
    }
    Row {
        anchors.right: parent.right
        anchors.rightMargin: 10
        anchors.verticalCenter: parent.verticalCenter
        anchors.verticalCenterOffset: root.description ? -9 : 0
        visible: root.toggle
        spacing: 6
        NacreText {
            anchors.verticalCenter: parent.verticalCenter
            text: root.stateText || (root.selected ? "On" : "Off")
            font.pointSize: 8
            color: NacreColours.palette.m3onSurfaceVariant
        }
        Rectangle {
            width: 28
            height: 16
            radius: 8
            color: root.selected ? NacreColours.palette.m3primary : NacreColours.palette.m3surfaceContainerHighest
            border.width: root.selected ? 0 : 1
            border.color: NacreColours.palette.m3outline
            Rectangle {
                width: 10
                height: 10
                radius: 5
                x: root.selected ? parent.width - width - 3 : 3
                y: 3
                color: root.selected ? NacreColours.palette.m3onPrimary : NacreColours.palette.m3onSurfaceVariant
            }
        }
    }
    NacreInteraction {
        function onClicked() {
            root.clicked();
        }

        accessibleName: (root.text || root.icon) + (root.toggle ? ". " + (root.stateText || (root.selected ? "On" : "Off")) + ". " + root.description : "")
    }
}
