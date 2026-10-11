import QtQuick
import qs.widgets
import qs.services

Column {
    id: root
    required property var monitor
    spacing: 6
    width: 388
    Level {
        label: "Volume"
        icon: NacreAudio.muted ? "volume_off" : "volume_up"
        level: NacreAudio.volume
        usable: NacreAudio.available
        canMute: true
        onAdjusted: value => NacreAudio.setVolume(value)
        onMute: NacreAudio.toggleMute()
    }
    Level {
        label: "Microphone"
        icon: NacreAudio.micMuted ? "mic_off" : "mic"
        level: NacreAudio.micVolume
        usable: NacreAudio.micAvailable
        canMute: true
        onAdjusted: value => NacreAudio.setMicVolume(value)
        onMute: NacreAudio.toggleMic()
    }
    Rectangle {
        width: parent.width
        height: 1
        color: Qt.alpha(NacreTokens.outline, .25)
        visible: display.visible || keyboard.visible
    }
    Level {
        id: display
        label: "Display"
        icon: "brightness_6"
        level: root.monitor?.brightness || 0
        usable: root.monitor?.available === true
        visible: usable
        onAdjusted: value => root.monitor?.setBrightness(value)
    }
    Level {
        id: keyboard
        label: "Keyboard"
        icon: "keyboard"
        level: NacreKeyboardLight.brightness
        usable: NacreKeyboardLight.available
        visible: usable
        onAdjusted: value => NacreKeyboardLight.setBrightness(value)
    }
    NacreText {
        width: parent.width
        text: root.monitor?.error || NacreKeyboardLight.error || ""
        visible: text !== ""
        maximumLineCount: 2
        wrapMode: Text.Wrap
        font.pointSize: 10
        color: NacreColours.palette.m3error
    }
    component Level: Item {
        id: row
        required property string label
        required property string icon
        required property real level
        required property bool usable
        property bool canMute: false
        signal adjusted(real value)
        signal mute
        width: root.width
        height: 50
        NacreIcon {
            x: 0
            y: 2
            text: row.icon
            font.pointSize: 12
            color: NacreTokens.mutedInk
        }
        NacreText {
            x: 26
            y: 0
            text: row.label
            font.pointSize: 10
        }
        NacreText {
            anchors.right: parent.right
            y: 0
            text: row.usable ? Math.round(row.level * 100) + "%" : "Unavailable"
            font.pointSize: 10
            color: NacreTokens.mutedInk
        }
        NacreRangeSlider {
            objectName: "control" + row.label
            x: 0
            y: 18
            width: parent.width - (row.canMute ? 44 : 0)
            height: 32
            label: row.label
            value: row.level
            enabled: row.usable
            onMoved: if (enabled)
                row.adjusted(value)
        }
        NacreSurface {
            objectName: "mute" + row.label
            anchors.right: parent.right
            y: 17
            width: 32
            height: 32
            radius: 16
            visible: row.canMute
            enabled: row.usable
            color: "transparent"
            NacreIcon {
                anchors.centerIn: parent
                text: row.icon
                font.pointSize: 14
            }
            NacreInteraction {
                accessibleName: "Toggle " + row.label + " mute"
                function onClicked() {
                    row.mute();
                }
            }
        }
    }
}
