import QtQuick
import QtQuick.Controls

Slider {
    id: root
    implicitHeight: 32
    background: Rectangle {
        x: root.leftPadding
        y: root.topPadding + (root.availableHeight - height) / 2
        width: root.availableWidth
        height: 4
        radius: 2
        color: NacreTokens.raised
        Rectangle {
            width: root.visualPosition * parent.width
            height: parent.height
            radius: 2
            color: NacreTokens.accent
        }
    }
    handle: Rectangle {
        x: root.leftPadding + root.visualPosition * (root.availableWidth - width)
        y: root.topPadding + (root.availableHeight - height) / 2
        width: 16
        height: 16
        radius: 8
        color: NacreTokens.accent
        border.width: root.activeFocus ? 2 : 0
        border.color: NacreTokens.ink
    }
}
