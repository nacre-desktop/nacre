import QtQuick
import Quickshell.Services.Pipewire
import qs.widgets
import qs.services

Item {
    id: root
    readonly property var candidates: Pipewire.nodes.values.filter(node => !node.isStream && node.isSink)
    readonly property var outputs: candidates.filter(node => !!node.audio)
    implicitWidth: 352
    implicitHeight: body.implicitHeight
    function chooseOutput(node) {
        if (!node || !outputs.includes(node) || !node.ready || !node.audio)
            return false;
        Pipewire.preferredDefaultAudioSink = node;
        return true;
    }
    PwObjectTracker {
        objects: root.candidates
    }
    Column {
        id: body
        width: root.width
        spacing: 10
        NacreText {
            text: NacreI18n.tr("popouts.sound-popup.text.sound", "Sound")
            font.pointSize: 14
        }
        Row {
            width: parent.width
            spacing: 10
            NacreText {
                width: Math.max(0, parent.width - outputMute.width - parent.spacing)
                anchors.verticalCenter: parent.verticalCenter
                text: NacreAudio.sink?.description || NacreI18n.tr("popouts.sound-popup.text.output-unavailable", "Output unavailable")
                elide: Text.ElideMiddle
            }
            ActionButton {
                id: outputMute
                text: NacreAudio.muted ? NacreI18n.tr("popouts.sound-popup.text.unmute", "Unmute") : NacreI18n.tr("popouts.sound-popup.text.mute", "Mute")
                enabled: NacreAudio.available
                compact: true
                onClicked: if (enabled)
                    NacreAudio.toggleMute()
            }
        }
        NacreQuickSlider {
            objectName: "quickOutputVolume"
            width: parent.width
            label: NacreI18n.tr("popouts.sound-popup.label.output-volume", "Output volume")
            value: NacreAudio.volume
            enabled: NacreAudio.available
            onMoved: if (enabled)
                NacreAudio.setVolume(value)
        }
        Row {
            width: parent.width
            spacing: 10
            NacreText {
                width: Math.max(0, parent.width - micMute.width - parent.spacing)
                anchors.verticalCenter: parent.verticalCenter
                text: NacreAudio.micAvailable ? NacreI18n.tr("popouts.sound-popup.text.microphone", "Microphone") : NacreI18n.tr("popouts.sound-popup.text.microphone-unavailable", "Microphone unavailable")
                elide: Text.ElideRight
            }
            ActionButton {
                id: micMute
                objectName: "quickMicMute"
                text: NacreAudio.micMuted ? NacreI18n.tr("popouts.sound-popup.text.unmute", "Unmute") : NacreI18n.tr("popouts.sound-popup.text.mute", "Mute")
                enabled: NacreAudio.micAvailable
                compact: true
                onClicked: if (enabled)
                    NacreAudio.toggleMic()
            }
        }
        NacreQuickSlider {
            objectName: "quickMicVolume"
            width: parent.width
            label: NacreI18n.tr("popouts.sound-popup.label.microphone-volume", "Microphone volume")
            value: NacreAudio.micVolume
            enabled: NacreAudio.micAvailable
            onMoved: if (enabled)
                NacreAudio.setMicVolume(value)
        }
        NacreQuickList {
            width: parent.width
            Column {
                width: parent.width
                spacing: 6
                Repeater {
                    model: root.outputs
                    delegate: NacreSurface {
                        required property var modelData
                        readonly property bool selected: Pipewire.defaultAudioSink === modelData
                        width: parent.width - 10
                        height: 44
                        radius: 12
                        enabled: modelData.ready
                        opacity: enabled ? 1 : 0.45
                        color: selected ? Qt.alpha(NacreTokens.accent, 0.12) : "transparent"
                        NacreIcon {
                            x: 10
                            anchors.verticalCenter: parent.verticalCenter
                            text: parent.selected ? "check" : "speaker"
                            font.pointSize: 14
                            color: parent.selected ? NacreTokens.accent : NacreTokens.mutedInk
                        }
                        NacreText {
                            x: 40
                            y: 10
                            width: parent.width - 50
                            height: parent.height - 20
                            text: parent.modelData.description || parent.modelData.name || NacreI18n.tr("popouts.sound-popup.text.audio-output", "Audio output")
                            verticalAlignment: Text.AlignVCenter
                            elide: Text.ElideMiddle
                            font.pointSize: 11
                            color: NacreTokens.ink
                        }
                        NacreInteraction {
                            accessibleName: NacreI18n.tr("ui.sound-popup.accessibleName.use-output-value0", "Use output {value0}", {
                                value0: (parent.modelData.description || parent.modelData.name || NacreI18n.tr("popouts.sound-popup.accessibleName.audio-output", "Audio output"))
                            })
                            function onClicked() {
                                root.chooseOutput(parent.modelData);
                            }
                        }
                    }
                }
            }
        }
    }
}
