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

    implicitHeight: Math.max(34, label.implicitHeight + NacreTokens.space.md)
    implicitWidth: label.implicitWidth + (compact ? NacreTokens.space.xl : NacreTokens.space.huge) + (icon ? (compact ? 22 : 25) : 0)
    radius: NacreTokens.radii.full
    color: selected ? NacreColours.palette.m3primary : NacreColours.palette.m3surfaceContainerHigh
    opacity: enabled ? 1 : 0.4

    Row {
        anchors.centerIn: parent
        width: root.width < root.implicitWidth ? parent.width - (root.compact ? NacreTokens.space.xl : NacreTokens.space.huge) : implicitWidth
        spacing: root.compact ? NacreTokens.space.xs : NacreTokens.space.sm

        NacreIcon {
            anchors.verticalCenter: label.lineCount > 1 ? parent.verticalCenter : undefined
            text: root.icon
            visible: root.icon.length > 0
            font.pointSize: root.compact ? NacreTokens.iconSize.small : NacreTokens.iconSize.medium
            color: root.selected ? NacreColours.palette.m3onPrimary : NacreColours.palette.m3onSurface
        }

        NacreText {
            id: label

            text: root.text
            width: root.width < root.implicitWidth ? parent.width - (root.icon ? (root.compact ? 22 : 25) : 0) : implicitWidth
            wrapMode: Text.Wrap
            font.pointSize: root.compact ? NacreTokens.textSize.caption : NacreTokens.textSize.body
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
