import QtQuick
import QtQuick.Controls.Basic
import qs.services

Switch {
    id: root
    property bool dimWhenDisabled: true
    opacity: enabled || !dimWhenDisabled ? 1 : .45
    implicitWidth: NacreTokens.space.huge * 2
    implicitHeight: NacreTokens.space.huge + NacreTokens.space.sm
    indicator: Rectangle {
        implicitWidth: NacreTokens.space.xxl * 2
        implicitHeight: NacreTokens.space.huge
        x: root.width - width - root.rightPadding
        y: (root.height - height) / 2
        radius: NacreTokens.radii.full
        color: root.checked ? NacreColours.palette.m3primaryContainer : NacreColours.palette.m3surfaceContainerHighest
        border.width: 1
        border.color: root.visualFocus ? NacreTokens.ink : NacreTokens.outline
        Rectangle {
            width: NacreTokens.space.xxl
            height: width
            x: root.checked ? parent.width - width - NacreTokens.space.xs : NacreTokens.space.xs
            y: NacreTokens.space.xs
            radius: NacreTokens.radii.full
            color: root.checked ? NacreColours.palette.m3onPrimaryContainer : NacreTokens.mutedInk
        }
    }
}
