import QtQuick
import QtTest
import "fixtures"

TestCase {
    id: test
    name: "NacreDefaultAudio"
    when: windowShown
    Component {
        id: service
        NacreAudio {}
    }
    Component {
        id: device
        QtObject {
            property bool ready: true
            property QtObject audio: QtObject {
                property real volume: .6
                property bool muted: true
            }
        }
    }
    function test_native_changes_and_default_switch_do_not_write_or_transfer_requests() {
        const first = createTemporaryObject(device, test);
        const second = createTemporaryObject(device, test);
        Pipewire.nodes = {
            values: [first, second]
        };
        Pipewire.defaultAudioSink = first;
        Pipewire.defaultAudioSource = first;
        const audio = createTemporaryObject(service, test);
        verify(audio.available);
        verify(audio.micAvailable);
        verify(audio.muted);
        verify(audio.micMuted);
        compare(audio.volume, .6);
        first.audio.volume = .4;
        compare(audio.volume, .4);
        audio.setVolume(NaN);
        audio.setMicVolume(Infinity);
        compare(first.audio.volume, .4);
        audio.setVolume(2);
        compare(first.audio.volume, 1);
        verify(first.audio.muted);
        audio.setMicVolume(-1);
        compare(first.audio.volume, 0);
        verify(first.audio.muted);
        Pipewire.defaultAudioSink = second;
        Pipewire.defaultAudioSource = second;
        compare(audio.volume, .6);
        compare(audio.micVolume, .6);
        verify(second.audio.muted);
        audio.setVolume(.1, first);
        audio.setMicVolume(.1, first);
        compare(first.audio.volume, 0);
        compare(second.audio.volume, .6);
        second.ready = false;
        verify(!audio.available);
        verify(!audio.micAvailable);
        audio.toggleMute();
        audio.toggleMic();
        audio.setVolume(.2);
        verify(second.audio.muted);
        compare(second.audio.volume, .6);
        second.ready = true;
        audio.toggleMic();
        verify(!second.audio.muted);
        Pipewire.nodes = {
            values: []
        };
        verify(!audio.available);
        audio.setVolume(.3);
        compare(second.audio.volume, .6);
        Pipewire.defaultAudioSink = null;
        Pipewire.defaultAudioSource = null;
        compare(audio.volume, 0);
        verify(!audio.micAvailable);
    }
}
