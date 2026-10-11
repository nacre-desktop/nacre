pragma Singleton
import Quickshell

// External configuration compatibility; implementation belongs to NacreAudio.
Singleton {
    readonly property var sink: NacreAudio.sink
    readonly property var source: NacreAudio.source
    readonly property bool muted: NacreAudio.muted
    readonly property real volume: NacreAudio.volume
    readonly property bool micMuted: NacreAudio.micMuted
    readonly property real micVolume: NacreAudio.micVolume
    readonly property bool micAvailable: NacreAudio.micAvailable
    function setVolume(value) {
        NacreAudio.setVolume(value);
    }
    function setMicVolume(value) {
        NacreAudio.setMicVolume(value);
    }
    function toggleMute() {
        NacreAudio.toggleMute();
    }
    function toggleMic() {
        NacreAudio.toggleMic();
    }
}
