pragma Singleton
import Quickshell
import Quickshell.Services.Pipewire

Singleton {
    id: root
    readonly property var sink: Pipewire.defaultAudioSink
    readonly property var outputs: Pipewire.nodes.values.filter(node => node.isSink && !node.isStream && !!node.audio && !/^nacre_record_[0-9a-f]{32}$/.test(node.name || ""))
    readonly property var source: Pipewire.defaultAudioSource
    readonly property var playbackStreams: Pipewire.nodes.values.filter(node => node.isStream && node.isSink && !!node.audio)
    readonly property bool available: usable(sink)
    readonly property bool micAvailable: usable(source)
    readonly property bool muted: available ? sink.audio.muted : false
    readonly property real volume: available ? sink.audio.volume : 0
    readonly property bool micMuted: micAvailable ? source.audio.muted : false
    readonly property real micVolume: micAvailable ? source.audio.volume : 0
    PwObjectTracker {
        objects: [...root.outputs, ...root.playbackStreams, root.source].filter(node => !!node)
    }
    function usable(node) {
        return !!node && Pipewire.nodes.values.includes(node) && node.ready && !!node.audio;
    }
    function setVolume(value, target = sink) {
        if (target === sink && usable(target) && Number.isFinite(value))
            target.audio.volume = Math.max(0, Math.min(1, value));
    }
    function setMicVolume(value, target = source) {
        if (target === source && usable(target) && Number.isFinite(value))
            target.audio.volume = Math.max(0, Math.min(1, value));
    }
    function chooseOutput(id) {
        const node = outputs.find(item => String(item.id) === id);
        if (!usable(node))
            return false;
        Pipewire.preferredDefaultAudioSink = node;
        return true;
    }
    function toggleMute() {
        if (available)
            sink.audio.muted = !sink.audio.muted;
    }
    function toggleMic() {
        if (micAvailable)
            source.audio.muted = !source.audio.muted;
    }
}
