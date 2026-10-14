import QtQuick
import Quickshell
import qs.modules.controlcenter as Controls
import Quickshell.Services.Pipewire
import qs.widgets
import qs.services

Column {
    id: root
    property bool active: true
    readonly property var candidates: Pipewire.nodes.values
    readonly property var outputs: candidates.filter(node => !node.isStream && node.isSink && !!node.audio)
    readonly property var inputs: candidates.filter(node => !node.isStream && !node.isSink && !!node.audio)
    readonly property var streams: candidates.filter(node => node.isStream && !!node.audio)
    width: parent?.width ?? 352
    spacing: 12
    function chooseOutput(node) {
        if (!node || !outputs.includes(node) || !node.ready || !node.audio)
            return false;
        Pipewire.preferredDefaultAudioSink = node;
        return true;
    }
    function chooseInput(node) {
        if (!node || !inputs.includes(node) || !node.ready || !node.audio)
            return false;
        Pipewire.preferredDefaultAudioSource = node;
        return true;
    }
    function setStream(node, value) {
        if (!streams.includes(node) || !node.ready || !node.audio || !Number.isFinite(value))
            return false;
        node.audio.volume = Math.max(0, Math.min(1, value));
        return true;
    }
    function muteStream(node) {
        if (!streams.includes(node) || !node.ready || !node.audio)
            return false;
        node.audio.muted = !node.audio.muted;
        return true;
    }
    PwObjectTracker {
        objects: root.candidates
    }
    Column {
        width: parent.width
        spacing: 4
        NacreText {
            text: NacreI18n.tr("controlcenter.sound.outputs", "Output")
            font.pointSize: 12
            font.weight: Font.Medium
        }
        Repeater {
            model: root.outputs
            delegate: Device {
                required property var modelData
                node: modelData
                selected: Pipewire.defaultAudioSink === node
                onChosen: root.chooseOutput(node)
            }
        }
        NacreText {
            visible: !root.outputs.length
            text: NacreI18n.tr("popouts.sound-popup.text.output-unavailable", "Output unavailable")
            font.pointSize: 10
            color: NacreTokens.mutedInk
        }
        LevelRow {
            label: NacreAudio.sink?.description || NacreI18n.tr("popouts.sound-popup.text.output-unavailable", "Output unavailable")
            accessibleLabel: NacreI18n.tr("popouts.sound-popup.label.output-volume", "Output volume")
            value: NacreAudio.volume
            muted: NacreAudio.muted
            usable: NacreAudio.available
            controlName: "quickOutputVolume"
            onAdjusted: value => NacreAudio.setVolume(value)
            onMuteRequested: NacreAudio.toggleMute()
        }
    }
    Column {
        width: parent.width
        spacing: 4
        NacreText {
            text: NacreI18n.tr("controlcenter.sound.inputs", "Input")
            font.pointSize: 12
            font.weight: Font.Medium
        }
        Repeater {
            model: root.inputs
            delegate: Device {
                required property var modelData
                node: modelData
                input: true
                selected: Pipewire.defaultAudioSource === node
                onChosen: root.chooseInput(node)
            }
        }
        NacreText {
            visible: !root.inputs.length
            text: NacreI18n.tr("popouts.sound-popup.text.microphone-unavailable", "Microphone unavailable")
            font.pointSize: 10
            color: NacreTokens.mutedInk
        }
        LevelRow {
            label: NacreAudio.source?.description || NacreI18n.tr("popouts.sound-popup.text.microphone-unavailable", "Microphone unavailable")
            accessibleLabel: NacreI18n.tr("popouts.sound-popup.label.microphone-volume", "Microphone volume")
            value: NacreAudio.micVolume
            muted: NacreAudio.micMuted
            input: true
            usable: NacreAudio.micAvailable
            controlName: "quickMicVolume"
            onAdjusted: value => NacreAudio.setMicVolume(value)
            onMuteRequested: NacreAudio.toggleMic()
        }
        PwNodePeakMonitor {
            id: inputPeak
            objectName: "controlInputPeak"
            node: NacreAudio.source
            enabled: root.active && root.visible && NacreAudio.micAvailable && !NacreAudio.micMuted
        }
        Row {
            width: parent.width
            spacing: 12
            NacreText {
                text: NacreI18n.tr("controlcenter.sound.input-level", "Input level")
                font.pointSize: 8
                color: NacreTokens.mutedInk
            }
            Rectangle {
                anchors.verticalCenter: parent.verticalCenter
                width: Math.max(0, parent.width - 100)
                height: 5
                radius: 2.5
                color: NacreTokens.raised
                Accessible.role: Accessible.ProgressBar
                Accessible.name: NacreI18n.tr("controlcenter.sound.input-level", "Input level")
                Rectangle {
                    height: parent.height
                    width: parent.width * (inputPeak.enabled ? Math.max(0, Math.min(1, inputPeak.peak)) : 0)
                    radius: 2.5
                    color: NacreTokens.accent
                    Behavior on width {
                        NumberAnimation {
                            duration: NacreTokens.motionEnabled ? 60 : 0
                        }
                    }
                }
            }
        }
    }
    Column {
        width: parent.width
        spacing: 4
        NacreText {
            text: NacreI18n.tr("controlcenter.sound.apps", "Applications")
            font.pointSize: 12
            font.weight: Font.Medium
        }
        NacreText {
            width: parent.width
            visible: !root.streams.length
            text: NacreI18n.tr("controlcenter.sound.no-apps", "Applications appear here when they play or record audio.")
            wrapMode: Text.Wrap
            font.pointSize: 10
            color: NacreTokens.mutedInk
        }
        Repeater {
            model: root.streams
            delegate: NacreSurface {
                id: stream
                required property var modelData
                readonly property string name: modelData.properties?.["application.name"] || modelData.description || modelData.name || NacreI18n.tr("controlcenter.sound.application", "Application")
                objectName: "controlAudioStream" + modelData.id
                width: parent.width
                height: 64
                radius: 18
                color: Qt.alpha(NacreTokens.raised, .72)
                Image {
                    id: appIcon
                    x: 14
                    y: 13
                    width: 20
                    height: 20
                    source: stream.modelData.properties?.["application.icon-name"] ? Quickshell.iconPath(stream.modelData.properties["application.icon-name"], true) : ""
                    sourceSize: Qt.size(32, 32)
                }
                NacreIcon {
                    x: 14
                    y: 13
                    text: "apps"
                    font.pointSize: 15
                    visible: appIcon.status !== Image.Ready
                    color: NacreTokens.mutedInk
                }
                NacreText {
                    x: 44
                    y: 12
                    width: parent.width - 96
                    text: stream.name
                    font.pointSize: 11
                    elide: Text.ElideRight
                }
                Controls.NacreControlSlider {
                    objectName: "controlStreamVolume" + stream.modelData.id
                    x: 8
                    y: 32
                    width: parent.width - 104
                    label: NacreI18n.tr("controlcenter.sound.app-volume", "Volume for {name}", {
                        name: stream.name
                    })
                    value: stream.modelData.audio?.volume ?? 0
                    enabled: stream.modelData.ready
                    onMoved: root.setStream(stream.modelData, value)
                }
                NacreText {
                    anchors.right: mute.right
                    anchors.rightMargin: 42
                    y: 38
                    text: NacreI18n.number(Math.round((stream.modelData.audio?.volume ?? 0) * 100)) + "%"
                    font.pointSize: 9
                    color: NacreTokens.mutedInk
                }
                Controls.NacreControlButton {
                    id: mute
                    anchors.right: parent.right
                    anchors.rightMargin: 10
                    anchors.verticalCenter: parent.verticalCenter
                    icon: stream.modelData.audio?.muted ? "volume_off" : "volume_up"
                    label: NacreI18n.tr("controlcenter.sound.app-mute", "Toggle mute for {name}", {
                        name: stream.name
                    })
                    enabled: stream.modelData.ready
                    onClicked: root.muteStream(stream.modelData)
                }
            }
        }
    }
    component LevelRow: NacreSurface {
        id: level
        required property string label
        required property string accessibleLabel
        required property string controlName
        required property real value
        required property bool muted
        required property bool usable
        property bool input: false
        signal adjusted(real value)
        signal muteRequested
        width: parent.width
        height: 56
        radius: 18
        color: NacreTokens.raised
        NacreText {
            x: 46
            y: 8
            width: parent.width - 60
            text: level.label
            color: NacreTokens.mutedInk
            font.pointSize: 9
            elide: Text.ElideMiddle
        }
        Controls.NacreControlButton {
            x: 8
            y: 22
            objectName: level.input ? "quickMicMute" : "quickOutputMute"
            icon: level.input ? (level.muted ? "mic_off" : "mic") : level.muted ? "volume_off" : "volume_up"
            label: NacreI18n.tr("controlcenter.sound.device-mute", "Toggle mute for {name}", {
                name: level.label
            })
            enabled: level.usable
            ink: NacreTokens.accent
            onClicked: level.muteRequested()
        }
        Controls.NacreControlSlider {
            objectName: level.controlName
            x: 42
            y: 24
            width: parent.width - 104
            label: level.accessibleLabel
            value: level.value
            enabled: level.usable
            onMoved: level.adjusted(value)
        }
        NacreText {
            anchors.right: parent.right
            anchors.rightMargin: 14
            y: 32
            text: level.usable ? NacreI18n.number(Math.round(level.value * 100)) + "%" : "—"
            font.pointSize: 10
            color: NacreTokens.mutedInk
        }
    }
    component Device: NacreSurface {
        id: device
        required property var node
        property bool selected: false
        property bool input: false
        signal chosen
        width: parent.width
        height: 44
        radius: 18
        enabled: node.ready
        color: selected ? NacreColours.palette.m3primaryContainer : NacreTokens.raised
        NacreIcon {
            x: 14
            anchors.verticalCenter: parent.verticalCenter
            text: device.input ? "mic" : "speaker"
            font.pointSize: 17
            color: device.selected ? NacreColours.palette.m3onPrimaryContainer : NacreTokens.mutedInk
        }
        NacreText {
            x: 44
            width: parent.width - 80
            height: parent.height
            color: device.selected ? NacreColours.palette.m3onPrimaryContainer : NacreTokens.ink
            text: device.node.description || device.node.name || NacreI18n.tr("popouts.sound-popup.text.audio-output", "Audio output")
            verticalAlignment: Text.AlignVCenter
            font.pointSize: 11
            elide: Text.ElideMiddle
        }
        NacreIcon {
            anchors.right: parent.right
            anchors.rightMargin: 14
            anchors.verticalCenter: parent.verticalCenter
            text: "check"
            visible: device.selected
            font.pointSize: 15
            color: NacreColours.palette.m3onPrimaryContainer
        }
        NacreInteraction {
            accessibleName: NacreI18n.tr("controlcenter.sound.use-device", "Use {name}", {
                name: device.node.description || device.node.name
            })
            function onClicked() {
                device.chosen();
            }
        }
    }
}
