import QtQuick
import QtQuick.Controls.Basic
import qs.widgets
import qs.services

Switch {
    id: root
    property string label: ""
    Accessible.name: label
    implicitWidth: 46
    implicitHeight: 32
    indicator: Rectangle {
        x: 4
        y: (root.height - height) / 2
        width: 38
        height: 22
        radius: 11
        color: root.checked ? NacreColours.palette.m3primaryContainer : NacreColours.palette.m3surfaceContainerHighest
        border.width: root.visualFocus ? 2 : 1
        border.color: root.visualFocus ? NacreTokens.accent : Qt.alpha(NacreTokens.outline, .4)
        opacity: root.enabled ? 1 : .4
        Rectangle {
            x: root.checked ? parent.width - width - 3 : 3
            y: 3
            width: 16
            height: 16
            radius: 8
            color: root.checked ? NacreColours.palette.m3onPrimaryContainer : NacreTokens.mutedInk
            Behavior on x {
                NumberAnimation {
                    duration: NacreTokens.motionEnabled ? 120 : 0
                }
            }
        }
    }
    contentItem: Item {}
}
