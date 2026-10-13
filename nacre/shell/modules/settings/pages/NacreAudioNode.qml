import QtQuick
import qs.widgets
import qs.services
import Quickshell.Services.Pipewire
import "../../dashboard/media"

Column {
    id: root
    required property var node
    property var dragNode: null
    readonly property bool present: !!node && Pipewire.nodes.values.includes(node)
    readonly property bool ready: present && node.ready && !!node.audio
    readonly property string caption: node ? (node.description || node.nickname || node.name || NacreI18n.tr("ui.audio-node.caption.audio-device", "Audio device")) : NacreI18n.tr("ui.audio-node.caption.disconnected", "Disconnected")
    width: parent.width
    spacing: 8
    function setVolume(target, fraction) {
        if (target && target === node && Pipewire.nodes.values.includes(target) && target.ready && target.audio && Number.isFinite(fraction))
            target.audio.volume = Math.max(0, Math.min(1, fraction));
    }
    function toggleMute() {
        if (ready)
            node.audio.muted = !node.audio.muted;
    }
    function chooseDefault() {
        if (!ready || node.isStream)
            return;
        if (node.isSink)
            Pipewire.preferredDefaultAudioSink = node;
        else
            Pipewire.preferredDefaultAudioSource = node;
    }
    Row {
        width: parent.width
        spacing: 12
        NacreText {
            width: Math.max(0, parent.width - 112)
            text: root.caption
            wrapMode: Text.Wrap
            font.pointSize: 11
            anchors.verticalCenter: parent.verticalCenter
        }
        ActionButton {
            text: NacreI18n.tr("ui.audio-node.text.default", "Default")
            visible: root.node && !root.node.isStream
            enabled: root.ready
            selected: root.node && (root.node.isSink ? Pipewire.defaultAudioSink === root.node : Pipewire.defaultAudioSource === root.node)
            onClicked: root.chooseDefault()
        }
    }
    Row {
        width: parent.width
        spacing: 12
        NacreMediaButton {
            icon: root.ready && root.node.audio.muted ? "volume_off" : "volume_up"
            label: NacreI18n.tr("ui.audio-node.label.mute-value0", "Mute {value0}", {
                value0: root.caption
            })
            selected: root.ready && root.node.audio.muted
            available: root.ready
            onClicked: root.toggleMute()
        }
        NacreMediaSlider {
            objectName: "audioVolume"
            width: Math.max(0, parent.width - 118)
            anchors.verticalCenter: parent.verticalCenter
            enabled: root.ready
            label: NacreI18n.tr("ui.audio-node.label.value0-volume", "{value0} volume", {
                value0: root.caption
            })
            progress: root.ready ? root.node.audio.volume : 0
            onDragBegan: root.dragNode = root.node
            onDragEnded: fraction => {
                root.setVolume(root.dragNode, fraction);
                root.dragNode = null;
            }
            onKeyboardRequested: fraction => root.setVolume(root.node, fraction)
        }
        NacreText {
            width: 52
            anchors.verticalCenter: parent.verticalCenter
            text: root.ready ? NacreI18n.number(Math.round(root.node.audio.volume * 100)) + "%" : "—"
            font.pointSize: 10
            horizontalAlignment: Text.AlignRight
        }
    }
}
