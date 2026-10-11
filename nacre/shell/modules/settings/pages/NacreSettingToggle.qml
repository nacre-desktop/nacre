import QtQuick
import qs.widgets
import qs.services

Item {
    id: root
    property string label: ""
    property string setting: ""
    readonly property bool checked: DesktopSettings.data[setting] === true
    implicitHeight: Math.max(34, caption.implicitHeight)
    implicitWidth: 300
    width: parent?.width ?? implicitWidth
    NacreSurface {
        x: 0
        anchors.verticalCenter: parent.verticalCenter
        width: 28
        height: 28
        radius: 8
        color: root.checked ? NacreTokens.accent : NacreColours.palette.m3surfaceContainerHigh
        border.width: root.checked ? 0 : 1
        border.color: NacreTokens.outline
        NacreIcon {
            anchors.centerIn: parent
            text: "check"
            visible: root.checked
            font.pointSize: 14
            color: NacreTokens.focusInk(parent.color)
        }
    }
    NacreText {
        id: caption
        x: 42
        width: Math.max(0, parent.width - 42)
        anchors.verticalCenter: parent.verticalCenter
        text: root.label
        wrapMode: Text.Wrap
        font.pointSize: 11
    }
    NacreInteraction {
        radius: 8
        accessibleName: root.label
        Accessible.role: Accessible.CheckBox
        Accessible.checkable: true
        Accessible.checked: root.checked
        function onClicked() {
            if (root.setting)
                DesktopSettings.set(root.setting, !root.checked);
        }
    }
}
