import QtQuick
import QtQuick.Controls.Basic
import qs.widgets

Slider {
    id: root
    property string label: "Level"
    from: 0
    to: 1
    stepSize: 0.01
    padding: 4
    implicitWidth: 280
    implicitHeight: 30
    Accessible.name: label
    background: Rectangle {
        x: root.leftPadding
        y: (root.height - height) / 2
        width: root.availableWidth
        height: 6
        radius: 3
        color: NacreTokens.raised
        Rectangle {
            width: root.visualPosition * parent.width
            height: parent.height
            radius: 3
            color: NacreTokens.accent
        }
    }
    handle: Rectangle {
        x: root.leftPadding + root.visualPosition * (root.availableWidth - width)
        y: (root.height - height) / 2
        width: 18
        height: 18
        radius: 9
        color: root.pressed ? NacreTokens.ink : NacreTokens.accent
        border.width: root.activeFocus ? 2 : 0
        border.color: NacreTokens.outline
    }
}
