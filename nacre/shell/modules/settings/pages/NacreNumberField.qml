import QtQuick
import QtQuick.Controls.Basic
import qs.widgets

SpinBox {
    id: root
    property string label: ""
    signal adjusted(int number)
    implicitWidth: 180
    implicitHeight: 38
    editable: true
    wheelEnabled: false
    leftPadding: 38
    rightPadding: 38
    Accessible.name: label
    onValueModified: adjusted(value)
    background: NacreSurface {
        radius: 19
        color: NacreTokens.raised
        border.width: root.activeFocus ? 1 : 0
        border.color: NacreTokens.outline
    }
    contentItem: TextInput {
        text: root.textFromValue(root.value, root.locale)
        font.family: NacreTokens.textFamily
        font.pointSize: 11
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
        width: 34
        height: root.height
        radius: 17
        color: "transparent"
        NacreIcon {
            anchors.centerIn: parent
            text: "add"
            color: NacreTokens.accent
        }
    }
    down.indicator: NacreSurface {
        width: 34
        height: root.height
        radius: 17
        color: "transparent"
        NacreIcon {
            anchors.centerIn: parent
            text: "remove"
            color: NacreTokens.accent
        }
    }
}
