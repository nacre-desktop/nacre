import QtQuick
import QtQuick.Controls.Basic
import qs.widgets

SpinBox {
    id: root
    property string label: ""
    signal adjusted(int number)
    implicitWidth: NacreTokens.space.huge * 5 + NacreTokens.space.xl
    implicitHeight: NacreTokens.space.huge + NacreTokens.space.xs
    editable: true
    wheelEnabled: false
    leftPadding: NacreTokens.space.huge + NacreTokens.space.xs
    rightPadding: NacreTokens.space.huge + NacreTokens.space.xs
    Accessible.name: label
    onValueModified: adjusted(value)
    background: NacreSurface {
        radius: NacreTokens.space.xl
        color: NacreTokens.raised
        border.width: root.activeFocus ? 1 : 0
        border.color: NacreTokens.outline
    }
    contentItem: TextInput {
        text: root.textFromValue(root.value, root.locale)
        font.family: NacreTokens.textFamily
        font.pointSize: NacreTokens.textSize.body
        color: NacreTokens.ink
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        readOnly: !root.editable
        validator: root.validator
        inputMethodHints: Qt.ImhFormattedNumbersOnly
        selectionColor: NacreTokens.accent
        selectedTextColor: NacreTokens.focusInk(NacreTokens.accent)
    }
    up.indicator: NacreSurface {
        x: root.width - width
        width: NacreTokens.space.huge + NacreTokens.space.hairline
        height: root.height
        radius: NacreTokens.space.xl
        color: "transparent"
        NacreIcon {
            anchors.centerIn: parent
            text: "add"
            color: NacreTokens.accent
        }
    }
    down.indicator: NacreSurface {
        width: NacreTokens.space.huge + NacreTokens.space.hairline
        height: root.height
        radius: NacreTokens.space.xl
        color: "transparent"
        NacreIcon {
            anchors.centerIn: parent
            text: "remove"
            color: NacreTokens.accent
        }
    }
}
