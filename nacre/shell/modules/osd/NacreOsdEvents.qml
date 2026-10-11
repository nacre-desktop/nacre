import QtQuick
import qs.services
import qs.config

Item {
    id: root
    required property var screen
    required property var visibilities
    required property bool hovered
    readonly property var monitor: NacreBrightness.getMonitorForScreen(screen)
    property bool armed: false
    property bool audioBaseline: false
    property bool micBaseline: false
    readonly property bool micReady: NacreAudio.micAvailable
    readonly property bool audioReady: NacreAudio.available
    property bool shown: false
    property string channel: "volume"
    function show(kind = "volume") {
        if (!armed || visibilities.osd || visibilities.session || WallpaperPlayback.locked || screen.name !== NacreHyprland.focusedMonitor?.name)
            return;
        channel = kind;
        shown = true;
        expiry.restart();
    }
    onAudioReadyChanged: {
        audioBaseline = false;
        Qt.callLater(() => audioBaseline = audioReady);
    }
    onMicReadyChanged: {
        micBaseline = false;
        Qt.callLater(() => micBaseline = micReady);
    }
    Component.onCompleted: Qt.callLater(() => {
        armed = true;
        audioBaseline = audioReady;
        micBaseline = micReady;
    })
    Connections {
        target: NacreAudio
        function onVolumeChanged() {
            if (root.audioBaseline)
                root.show();
        }
        function onMutedChanged() {
            if (root.audioBaseline)
                root.show();
        }
    }
    Connections {
        target: NacreAudio
        function onMicVolumeChanged() {
            if (root.micBaseline)
                root.show("microphone");
        }
        function onMicMutedChanged() {
            if (root.micBaseline)
                root.show("microphone");
        }
    }
    Connections {
        target: root.monitor
        function onAdjusted() {
            root.show("display");
        }
    }
    Connections {
        target: NacreKeyboardLight
        function onAdjusted() {
            root.show("keyboard");
        }
    }
    Connections {
        target: root.visibilities
        function onOsdChanged() {
            if (root.visibilities.osd)
                root.shown = false;
        }
    }
    NacreLevelNotice {
        output: root.screen
        channel: root.channel
        shown: root.shown
    }
    onHoveredChanged: if (hovered)
        expiry.stop()
    else if (shown)
        expiry.restart()
    Timer {
        id: expiry
        interval: NacreOsd.hideDelay
        onTriggered: if (!root.hovered)
            root.shown = false
    }
}
