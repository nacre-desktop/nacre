import QtQuick
import QtQuick.Controls.Basic
import qs.widgets

Item {
    id: toggleRow
    property string label: ""
    property string description: ""
    property bool checked: false
    signal toggled(bool value)
    implicitHeight: Math.max(labels.implicitHeight, control.implicitHeight)
    implicitWidth: NacreTokens.space.huge * 9 + NacreTokens.space.md
    opacity: enabled ? 1 : .5
    Column {
        id: labels
        width: Math.max(0, toggleRow.width - control.width - NacreTokens.space.md)
        spacing: NacreTokens.space.xs
        NacreText {
            width: parent.width
            text: toggleRow.label
            wrapMode: Text.Wrap
            font.pointSize: NacreTokens.textSize.body
        }
        NacreText {
            visible: toggleRow.description.length > 0
            width: parent.width
            text: toggleRow.description
            wrapMode: Text.Wrap
            color: NacreTokens.mutedInk
            font.pointSize: NacreTokens.textSize.caption
        }
    }
    Switch {
        id: control
        objectName: "dashboardPreferenceToggle"
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        checked: toggleRow.checked
        Accessible.name: toggleRow.label
        onToggled: toggleRow.toggled(checked)
        indicator: NacreSurface {
            implicitWidth: NacreTokens.space.xxl + NacreTokens.space.xl + NacreTokens.space.xs
            implicitHeight: NacreTokens.space.xl + NacreTokens.space.sm
            x: parent.leftPadding
            y: (parent.height - height) / 2
            radius: NacreTokens.radii.full
            color: toggleRow.checked ? NacreTokens.accent : NacreTokens.body
            border.width: 1
            border.color: NacreTokens.outline
            NacreSurface {
                x: toggleRow.checked ? parent.width - width - NacreTokens.space.xs : NacreTokens.space.xs
                y: NacreTokens.space.xs
                width: NacreTokens.radii.large
                height: NacreTokens.radii.large
                radius: NacreTokens.radii.full
                color: toggleRow.checked ? NacreTokens.focusInk(NacreTokens.accent) : NacreTokens.mutedInk
            }
        }
    }
}
