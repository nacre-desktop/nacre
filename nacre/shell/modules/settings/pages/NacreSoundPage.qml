import QtQuick
import qs.widgets
import qs.services
import Quickshell.Services.Pipewire

NacreSettingsPage {
    id: root
    property string page: "sound"
    readonly property var audioNodes: [...Pipewire.nodes.values].filter(node => !!node.audio)
    PwObjectTracker {
        objects: root.audioNodes
    }
    Repeater {
        model: [
            {
                title: NacreI18n.tr("ui.sound-page.title.output-devices", "Output devices"),
                kind: "output"
            },
            {
                title: NacreI18n.tr("ui.sound-page.title.microphones", "Microphones"),
                kind: "input"
            },
            {
                title: NacreI18n.tr("ui.sound-page.title.application-audio", "Application audio"),
                kind: "stream"
            }
        ]
        delegate: NacreSettingsSection {
            id: section
            required property var modelData
            readonly property var devices: root.audioNodes.filter(node => modelData.kind === "stream" ? node.isStream : !node.isStream && node.isSink === (modelData.kind === "output"))
            title: modelData.title
            Repeater {
                model: section.devices
                delegate: NacreAudioNode {
                    required property var modelData
                    node: modelData
                }
            }
            NacreText {
                width: parent.width
                visible: section.devices.length === 0
                text: section.modelData.kind === "stream" ? NacreI18n.tr("ui.sound-page.text.no-applications-are-playing-or-recording-audio", "No applications are playing or recording audio.") : NacreI18n.tr("ui.sound-page.text.no-audio-devices-available", "No audio devices available.")
                color: NacreTokens.mutedInk
                wrapMode: Text.Wrap
            }
        }
    }
    NacreSettingsSection {
        title: NacreI18n.tr("ui.sound-page.title.advanced-audio", "Advanced audio")
        description: NacreI18n.tr("ui.sound-page.description.change-device-profiles-and-application-routing-in-the-a", "Change device profiles and application routing in the audio manager.")
        ActionButton {
            text: NacreI18n.tr("ui.sound-page.text.open-audio-manager", "Open audio manager")
            onClicked: DesktopActions.execute("audio")
        }
    }
}
