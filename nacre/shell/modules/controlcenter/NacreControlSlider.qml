import QtQuick
import QtQuick.Controls.Basic
import qs.widgets
import qs.services

Slider {
    id: root
    property string label: NacreI18n.tr("ui.range-slider.label.level", "Level")
    from: 0
    to: 1
    stepSize: .01
    live: true
    padding: 6
    implicitWidth: 240
    implicitHeight: 30
    Accessible.name: label
    background: Rectangle {
        x: root.leftPadding
        y: (root.height - height) / 2
        width: root.availableWidth
        height: 6
        radius: 3
        color: NacreColours.palette.m3surfaceContainerHighest
        Rectangle {
            width: root.visualPosition * parent.width
            height: parent.height
            radius: 3
            gradient: Gradient {
                orientation: Gradient.Horizontal
                GradientStop {
                    position: 0
                    color: NacreTokens.accent
                }
                GradientStop {
                    position: 1
                    color: NacreColours.palette.m3secondary
                }
            }
            opacity: root.enabled ? 1 : .35
        }
    }
    handle: Rectangle {
        x: root.leftPadding + root.visualPosition * (root.availableWidth - width)
        y: (root.height - height) / 2
        width: 8
        height: 16
        radius: 3
        color: NacreTokens.accent
        border.width: root.visualFocus ? 2 : 0
        border.color: NacreTokens.focusInk(NacreTokens.body)
        opacity: root.enabled ? 1 : .35
    }
}
