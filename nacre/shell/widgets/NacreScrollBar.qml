import QtQuick
import QtQuick.Controls.Basic

ScrollBar {
    id: root
    padding: 2
    hoverEnabled: true
    minimumSize: 0.06
    implicitWidth: horizontal ? 40 : 10
    implicitHeight: horizontal ? 10 : 40
    contentItem: NacreSurface {
        implicitWidth: root.horizontal ? 0 : 6
        implicitHeight: root.horizontal ? 6 : 0
        radius: Math.min(width, height) / 2
        color: root.pressed ? NacreTokens.accent : NacreTokens.outline
        opacity: root.size >= 1 ? 0 : root.active || root.hovered ? 0.85 : 0.35
    }
    background: Item {}
}
