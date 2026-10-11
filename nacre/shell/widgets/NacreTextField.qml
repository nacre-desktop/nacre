import QtQuick
import QtQuick.Controls.Basic

TextField {
    id: root
    property string accessibleName: placeholderText
    font.family: NacreTokens.textFamily
    font.pointSize: NacreTokens.textPointSize
    color: NacreTokens.ink
    placeholderTextColor: NacreTokens.mutedInk
    selectionColor: NacreTokens.accent
    selectedTextColor: NacreTokens.focusInk(selectionColor)
    selectByMouse: true
    hoverEnabled: true
    padding: 4
    Accessible.name: accessibleName
    background: NacreSurface {
        implicitWidth: 120
        implicitHeight: 24
        radius: NacreTokens.cornerRadius
        color: NacreTokens.raised
        border.width: root.activeFocus ? 1 : 0
        border.color: NacreTokens.outline
    }
}
