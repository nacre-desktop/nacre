import QtQuick
import Quickshell
import Quickshell.Wayland
import qs.widgets
import qs.services
import qs.config

NacreWindow {
    id: root
    required property var output
    property bool shown: false
    property string channel: "volume"
    screen: output
    name: "level-feedback"
    property real reveal: shown ? 1 : 0
    Connections {
        target: NacreTokens
        function onMotionEnabledChanged() {
            if (!NacreTokens.motionEnabled) {
                fadeMotion.stop();
                root.reveal = Qt.binding(() => root.shown ? 1 : 0);
            }
        }
    }
    visible: reveal > 0 && !NacrePanelState.hidden
    Behavior on reveal {
        enabled: NacreTokens.motionEnabled
        NumberAnimation {
            id: fadeMotion
            duration: 150
            easing.type: Easing.OutCubic
        }
    }
    anchors.bottom: true
    margins.bottom: NacreFrame.bottom + 24
    implicitWidth: Math.min(340, Math.max(200, output?.width - 48))
    implicitHeight: 80
    WlrLayershell.exclusionMode: ExclusionMode.Ignore
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
    mask: Region {
        width: 0
        height: 0
    }
    readonly property var monitor: NacreBrightness.getMonitorForScreen(output)
    readonly property real value: channel === "display" ? monitor?.brightness || 0 : channel === "keyboard" ? NacreKeyboardLight.brightness : channel === "microphone" ? NacreAudio.micVolume : NacreAudio.volume
    readonly property bool muted: channel === "volume" ? NacreAudio.muted : channel === "microphone" ? NacreAudio.micMuted : false
    NacreSurface {
        opacity: root.reveal
        transform: Translate {
            y: (1 - root.reveal) * 6
        }
        anchors.fill: parent
        radius: 22
        color: NacreTokens.body
        border.width: 1
        border.color: Qt.alpha(NacreTokens.outline, .5)
        NacreText {
            x: 18
            y: 12
            text: root.channel === "display" ? "Display brightness" : root.channel === "keyboard" ? "Keyboard brightness" : root.channel === "microphone" ? "Microphone" : "Volume"
            font.pointSize: 11
        }
        NacreText {
            anchors.right: parent.right
            anchors.rightMargin: 18
            y: 12
            text: root.muted ? "Muted" : Math.round(root.value * 100) + "%"
            font.pointSize: 11
            color: NacreTokens.mutedInk
        }
        Rectangle {
            x: 18
            y: 48
            width: parent.width - 36
            height: 8
            radius: 4
            color: NacreTokens.raised
            Rectangle {
                width: parent.width * root.value
                height: parent.height
                radius: 4
                color: NacreTokens.accent
                opacity: root.muted ? .35 : 1
            }
        }
    }
}
