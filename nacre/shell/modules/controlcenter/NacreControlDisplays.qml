import QtQuick
import qs.widgets
import qs.services

Column {
    id: root
    readonly property var displays: NacreBrightness.monitors.filter(monitor => monitor.available === true)
    visible: displays.length > 0 || NacreKeyboardLight.available
    spacing: 14
    NacreText {
        visible: root.displays.length > 0
        text: NacreI18n.tr("controlcenter.displays.title", "Display brightness")
        font.pointSize: 12
        font.weight: Font.Medium
    }
    Repeater {
        model: root.displays
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
                text: modelData.error || ""
                wrapMode: Text.Wrap
                color: NacreTokens.mutedInk
                font.pointSize: 10
            }
        }
    }
    NacreText {
        visible: NacreKeyboardLight.available
        text: NacreI18n.tr("controlcenter.displays.keyboard", "Keyboard backlight")
        font.pointSize: 12
    }
    NacreControlSlider {
        objectName: "controlKeyboardBrightness"
        visible: NacreKeyboardLight.available
        width: parent.width
        enabled: NacreKeyboardLight.available
        value: NacreKeyboardLight.brightness
        label: NacreI18n.tr("controlcenter.displays.keyboard", "Keyboard backlight")
        onMoved: if (enabled)
            NacreKeyboardLight.setBrightness(value)
    }
    NacreText {
        width: parent.width
        visible: NacreKeyboardLight.available
        text: NacreKeyboardLight.error || NacreI18n.number(Math.round(NacreKeyboardLight.brightness * 100)) + "%"
        color: NacreTokens.mutedInk
        font.pointSize: 10
        wrapMode: Text.Wrap
    }
}
