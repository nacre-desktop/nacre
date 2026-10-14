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
        radius: NacreTokens.radii.full
        color: NacreTokens.raised
        Rectangle {
            width: root.visualPosition * parent.width
            height: parent.height
            radius: NacreTokens.radii.full
            color: NacreTokens.accent
        }
    }
    handle: Rectangle {
        x: root.leftPadding + root.visualPosition * (root.availableWidth - width)
        y: root.topPadding + (root.availableHeight - height) / 2
        width: NacreTokens.space.lg
        height: NacreTokens.space.lg
        radius: NacreTokens.radii.full
        color: NacreTokens.accent
        border.width: root.activeFocus ? 2 : 0
        border.color: NacreTokens.ink
    }
}
