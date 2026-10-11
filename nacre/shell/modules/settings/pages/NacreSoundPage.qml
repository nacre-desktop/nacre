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
                title: "Output devices",
                kind: "output"
            },
            {
                title: "Microphones",
                kind: "input"
            },
            {
                title: "Application audio",
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
                text: section.modelData.kind === "stream" ? "No applications are playing or recording audio." : "No audio devices available."
                color: NacreTokens.mutedInk
                wrapMode: Text.Wrap
            }
        }
    }
    NacreSettingsSection {
        title: "Advanced audio"
        description: "Change device profiles and application routing in the audio manager."
        ActionButton {
            text: "Open audio manager"
            onClicked: DesktopActions.execute("audio")
        }
    }
}
