import QtQuick
import qs.widgets
import qs.services

Column {
    id: root
    spacing: 14
    NacreText {
        text: NacreI18n.tr("controlcenter.displays.title", "Display brightness")
        font.pointSize: 12
        font.weight: Font.Medium
    }
    Repeater {
        model: NacreBrightness.monitors
        delegate: Column {
            required property var modelData
            readonly property string displayName: modelData.modelData?.description || modelData.modelData?.name || modelData.name || NacreI18n.tr("controlcenter.levels.this-display", "This display")
            width: root.width
            spacing: 4
            NacreText {
                width: parent.width
                text: displayName
                elide: Text.ElideMiddle
                font.pointSize: 11
            }
            NacreControlSlider {
                width: parent.width
                value: modelData.brightness
                enabled: modelData.available === true
                label: NacreI18n.tr("controlcenter.displays.adjust", "Brightness for {name}", {
                    name: displayName
                })
                onMoved: if (enabled)
                    modelData.setBrightness(value)
            }
            NacreText {
                width: parent.width
                visible: text !== ""
                text: modelData.error || (!modelData.available ? NacreI18n.tr("controlcenter.levels.no-brightness", "No brightness control for this display") : "")
                wrapMode: Text.Wrap
                color: NacreTokens.mutedInk
                font.pointSize: 10
            }
        }
    }
    NacreText {
        width: parent.width
        visible: !NacreBrightness.monitors.length
        text: NacreI18n.tr("controlcenter.displays.none", "No controllable displays found.")
        color: NacreTokens.mutedInk
        wrapMode: Text.Wrap
    }
    NacreText {
        visible: NacreKeyboardLight.available
        text: NacreI18n.tr("controlcenter.control-levels.label.keyboard", "Keyboard")
        font.pointSize: 12
    }
    NacreControlSlider {
        width: parent.width
        visible: NacreKeyboardLight.available
        enabled: NacreKeyboardLight.available
        value: NacreKeyboardLight.brightness
        label: NacreI18n.tr("controlcenter.displays.keyboard", "Keyboard backlight")
        onMoved: if (enabled)
            NacreKeyboardLight.setBrightness(value)
    }
}
