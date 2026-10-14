import QtQuick
import qs.widgets
import qs.services

Item {
    id: root
    property string label: ""
    property string setting: ""
    property bool defaultChecked: false
    enabled: setting !== "livePreviews" || NacreCompositor.capabilities?.windowCapture !== false
    readonly property bool checked: (DesktopSettings.data[setting] ?? defaultChecked) === true
    opacity: enabled ? 1 : .5
    implicitHeight: Math.max(34, caption.implicitHeight)
    implicitWidth: NacreTokens.space.huge * 9 + NacreTokens.space.md
    width: parent?.width ?? implicitWidth
    NacreSurface {
        x: 0
        anchors.verticalCenter: parent.verticalCenter
        width: NacreTokens.space.xxl
        height: NacreTokens.space.xxl
        radius: NacreTokens.radii.small
        color: root.checked ? NacreTokens.accent : NacreColours.palette.m3surfaceContainerHigh
        border.width: root.checked ? 0 : 1
        border.color: NacreTokens.outline
        NacreIcon {
            anchors.centerIn: parent
            text: "check"
            visible: root.checked
            font.pointSize: NacreTokens.textSize.title
            color: NacreTokens.focusInk(parent.color)
        }
    }
    NacreText {
        id: caption
        x: NacreTokens.space.huge + NacreTokens.space.compact
        width: Math.max(0, parent.width - 42)
        anchors.verticalCenter: parent.verticalCenter
        text: root.label + (root.setting === "livePreviews" && !root.enabled ? NacreI18n.tr("ui.compositor.unavailable-suffix", " — unavailable on this compositor") : "")
        wrapMode: Text.Wrap
        font.pointSize: NacreTokens.textSize.body
    }
    NacreInteraction {
        radius: NacreTokens.radii.small
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
