import QtQuick
import qs.widgets
import qs.services

Column {
    id: root
    required property var monitor
    signal detailsRequested(string section)
    spacing: 8
    width: 388
    Level {
        objectName: "controlVolumeRow"
        controlId: "Volume"
        label: NacreI18n.tr("controlcenter.control-levels.label.volume", "Volume")
        detailsLabel: NacreI18n.tr("controlcenter.levels.sound-details", "Sound and microphone controls")
        subtitle: NacreAudio.sink?.description || NacreI18n.tr("popouts.sound-popup.text.output-unavailable", "Output unavailable")
        icon: NacreAudio.muted ? "volume_off" : NacreAudio.volume < .35 ? "volume_down" : "volume_up"
        iconActionName: NacreAudio.muted ? NacreI18n.tr("controlcenter.levels.unmute", "Unmute output") : NacreI18n.tr("controlcenter.levels.mute", "Mute output")
        level: NacreAudio.volume
        usable: NacreAudio.available
        onAdjusted: value => NacreAudio.setVolume(value)
        onIconClicked: NacreAudio.toggleMute()
        onDetails: root.detailsRequested("audio")
    }
    Level {
        objectName: "controlBrightnessRow"
        controlId: "Brightness"
        label: NacreI18n.tr("controlcenter.levels.brightness", "Brightness")
        detailsLabel: NacreI18n.tr("controlcenter.levels.light-details", "Display and keyboard brightness")
        subtitle: root.monitor?.available ? root.monitor?.modelData?.description || root.monitor?.modelData?.name || root.monitor?.name || NacreI18n.tr("controlcenter.levels.this-display", "This display") : NacreI18n.tr("controlcenter.levels.no-brightness", "No brightness control for this display")
        icon: "brightness_6"
        iconActionName: NacreI18n.tr("controlcenter.displays.title", "Display brightness")
        level: root.monitor?.brightness || 0
        usable: root.monitor?.available === true
        onAdjusted: value => root.monitor?.setBrightness(value)
        onIconClicked: root.detailsRequested("battery")
        onDetails: root.detailsRequested("battery")
    }
    component Level: NacreSurface {
        id: row
        required property string controlId
        required property string iconActionName
        required property string label
        required property string detailsLabel
        required property string subtitle
        required property string icon
        required property real level
        required property bool usable
        signal adjusted(real value)
        signal iconClicked
        signal details
        width: root.width
        height: 56
        radius: 18
        color: Qt.alpha(NacreTokens.raised, .65)
        NacreText {
            x: 46
            y: 7
            width: parent.width - 132
            text: row.subtitle
            font.pointSize: 9
            color: NacreTokens.mutedInk
            elide: Text.ElideMiddle
        }
        NacreControlButton {
            x: 8
            y: 17
            icon: row.icon
            label: row.iconActionName
            enabled: row.usable
            ink: NacreTokens.accent
            onClicked: row.iconClicked()
        }
        NacreControlSlider {
            objectName: "control" + row.controlId
            x: 42
            y: 23
            width: parent.width - 130
            label: row.label
            value: row.level
            enabled: row.usable
            onMoved: if (enabled)
                row.adjusted(value)
        }
        NacreText {
            anchors.right: parent.right
            anchors.rightMargin: 40
            y: 29
            text: row.usable ? NacreI18n.number(Math.round(row.level * 100)) + "%" : "—"
            font.pointSize: 10
            color: NacreTokens.mutedInk
        }
        NacreControlButton {
            anchors.right: parent.right
            anchors.rightMargin: 4
            y: 22
            icon: "chevron_right"
            label: row.detailsLabel
            onClicked: row.details()
        }
    }
}
