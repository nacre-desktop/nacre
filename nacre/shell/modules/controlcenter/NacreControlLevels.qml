import QtQuick
import qs.widgets
import qs.services

NacreSurface {
    id: root
    required property var monitor
    property string kind: "volume"
    signal detailsRequested(string section, bool pointerOwned)
    readonly property bool sound: kind === "volume" || kind === "mic-volume"
    readonly property string controlId: ({
            volume: "Volume",
            brightness: "Brightness",
            "mic-volume": "Microphone",
            keyboard: "Keyboard"
        })[kind]
    readonly property bool usable: kind === "volume" ? NacreAudio.available : kind === "mic-volume" ? NacreAudio.micAvailable : kind === "keyboard" ? NacreKeyboardLight.available : monitor?.available === true
    readonly property real level: kind === "volume" ? NacreAudio.volume : kind === "mic-volume" ? NacreAudio.micVolume : kind === "keyboard" ? NacreKeyboardLight.brightness : monitor?.brightness || 0
    readonly property string label: kind === "volume" ? NacreI18n.tr("controlcenter.control-levels.label.volume", "Volume") : kind === "mic-volume" ? NacreI18n.tr("controlcenter.catalog.mic-volume", "Microphone volume") : kind === "keyboard" ? NacreI18n.tr("controlcenter.displays.keyboard", "Keyboard backlight") : NacreI18n.tr("controlcenter.levels.brightness", "Brightness")
    readonly property string detailsLabel: sound ? NacreI18n.tr("controlcenter.levels.sound-details", "Sound and microphone controls") : NacreI18n.tr("controlcenter.levels.light-details", "Display and keyboard brightness")
    readonly property string subtitle: kind === "volume" ? NacreAudio.sink?.description || NacreI18n.tr("popouts.sound-popup.text.output-unavailable", "Output unavailable") : kind === "mic-volume" ? NacreAudio.source?.description || "" : kind === "keyboard" ? label : monitor?.modelData?.description || monitor?.modelData?.name || monitor?.name || NacreI18n.tr("controlcenter.levels.this-display", "This display")
    readonly property string icon: kind === "volume" ? NacreAudio.muted ? "volume_off" : NacreAudio.volume < .35 ? "volume_down" : "volume_up" : kind === "mic-volume" ? NacreAudio.micMuted ? "mic_off" : "mic" : kind === "keyboard" ? "keyboard" : "brightness_6"
    readonly property string iconActionName: kind === "volume" ? NacreAudio.muted ? NacreI18n.tr("controlcenter.levels.unmute", "Unmute output") : NacreI18n.tr("controlcenter.levels.mute", "Mute output") : kind === "mic-volume" ? NacreAudio.micMuted ? NacreI18n.tr("controlcenter.catalog.unmute-mic", "Unmute microphone") : NacreI18n.tr("controlcenter.catalog.mute-mic", "Mute microphone") : label
    objectName: "control" + controlId + "Row"
    visible: kind === "volume" || usable
    width: 388
    implicitHeight: 56
    height: implicitHeight
    radius: 18
    color: Qt.alpha(NacreTokens.raised, .65)
    function adjust(value) {
        if (!usable)
            return;
        if (kind === "volume")
            NacreAudio.setVolume(value);
        else if (kind === "mic-volume")
            NacreAudio.setMicVolume(value);
        else if (kind === "keyboard")
            NacreKeyboardLight.setBrightness(value);
        else
            monitor.setBrightness(value);
    }
    function pressIcon(pointerOwned) {
        if (!usable)
            return;
        if (kind === "volume")
            NacreAudio.toggleMute();
        else if (kind === "mic-volume")
            NacreAudio.toggleMic();
        else
            detailsRequested("battery", pointerOwned);
    }
    NacreText {
        x: 46
        y: 7
        width: parent.width - 132
        text: root.subtitle
        font.pointSize: 9
        color: NacreTokens.mutedInk
        elide: Text.ElideMiddle
    }
    NacreControlButton {
        x: 8
        y: 17
        icon: root.icon
        label: root.iconActionName
        enabled: root.usable
        ink: NacreTokens.accent
        onClicked: root.pressIcon(pointerActivation)
    }
    NacreControlSlider {
        objectName: "control" + root.controlId
        x: 42
        y: 23
        width: parent.width - 130
        label: root.label
        value: root.level
        enabled: root.usable
        onMoved: if (enabled)
            root.adjust(value)
    }
    NacreText {
        anchors.right: parent.right
        anchors.rightMargin: 40
        y: 29
        text: root.usable ? NacreI18n.number(Math.round(root.level * 100)) + "%" : "—"
        font.pointSize: 10
        color: NacreTokens.mutedInk
    }
    NacreControlButton {
        anchors.right: parent.right
        anchors.rightMargin: 4
        y: 22
        icon: "chevron_right"
        label: root.detailsLabel
        onClicked: root.detailsRequested(root.sound ? "audio" : "battery", pointerActivation)
    }
}
