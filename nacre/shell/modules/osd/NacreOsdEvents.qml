import QtQuick
import Quickshell.Services.UPower
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
    property bool profileBaseline: false
    property bool nightBaseline: false
    property bool lastNight: false
    property var previousSink: null
    property bool outputSwitchPending: false
    readonly property bool micReady: NacreAudio.micAvailable
    readonly property bool audioReady: NacreAudio.available
    readonly property bool powerReady: NacrePowerPolicy.available
    readonly property bool nightReady: NacreControlTools.data.nightLightSupported === true && typeof NacreControlTools.data.nightLightEnabled === "boolean"
    property bool shown: false
    property string channel: "volume"
    readonly property alias notice: notice
    property var osdTopic: null
    property string pendingChannel: ""
    function allowed() {
        return armed && !visibilities.osd && !visibilities.session && !WallpaperPlayback.locked && !NacrePanelState.hidden && screen.name === NacrePanelState.activeName();
    }
    function cancel() {
        shown = false;
        pendingChannel = "";
        expiry.stop();
    }
    function presentPending() {
        if (!pendingChannel)
            return;
        if (!allowed()) {
            pendingChannel = "";
            return;
        }
        channel = pendingChannel;
        pendingChannel = "";
        shown = true;
        expiry.restart();
    }
    function show(kind = "volume") {
        if (!allowed())
            return;
        if ((["volume", "output"].includes(kind) && !audioReady) || (kind === "microphone" && !micReady) || (kind === "display" && !monitor?.available) || (kind === "keyboard" && !NacreKeyboardLight.available) || (kind === "night-light" && !nightReady) || (kind === "power-profile" && !powerReady) || (kind === "capslock" && !NacreCapsLock.ready))
            return;
        // Default-device rebinding can also notify volume; the device wins.
        if (pendingChannel !== "output" || kind === "output")
            pendingChannel = kind;
        Qt.callLater(presentPending);
    }
    function rearmAudio() {
        audioBaseline = audioReady;
        if (outputSwitchPending && audioReady) {
            outputSwitchPending = false;
            show("output");
        }
    }
    Component.onCompleted: if (NacreDaemon.enabled)
        osdTopic = NacreDaemon.subscribe("osd")
    Component.onDestruction: if (osdTopic)
        NacreDaemon.unsubscribe("osd")
    Connections {
        target: NacreDaemon
        function onEvent(topic, name, data) {
            if (topic !== "osd" || name !== "changed" || !data)
                return;
            if (data.kind === "volume") {
                NacreAudio.setOsdVolume(data.value);
                root.show();
            } else if (data.kind === "brightness") {
                NacreBrightness.applyDaemonLevel(data.device, data.value);
                root.show("display");
            } else if (data.kind === "keyboard") {
                NacreKeyboardLight.applyDaemonLevel(data.value, data.max);
                root.show("keyboard");
            }
        }
    }
    onAudioReadyChanged: {
        audioBaseline = false;
        Qt.callLater(rearmAudio);
    }
    onMicReadyChanged: {
        micBaseline = false;
        Qt.callLater(() => micBaseline = micReady);
    }
    onPowerReadyChanged: {
        profileBaseline = false;
        Qt.callLater(() => profileBaseline = powerReady);
    }
    onNightReadyChanged: {
        nightBaseline = false;
        Qt.callLater(() => nightBaseline = nightReady);
    }
    Timer {
        interval: 400
        running: true
        onTriggered: {
            root.previousSink = NacreAudio.sink;
            root.audioBaseline = root.audioReady;
            root.micBaseline = root.micReady;
            root.profileBaseline = root.powerReady;
            root.nightBaseline = root.nightReady;
            root.lastNight = NacreControlTools.data.nightLightEnabled === true;
            root.armed = true;
        }
    }
    Connections {
        target: NacreAudio
        function onSinkChanged() {
            const previous = root.previousSink;
            root.previousSink = NacreAudio.sink;
            root.audioBaseline = false;
            root.outputSwitchPending = !!previous && !!NacreAudio.sink && previous !== NacreAudio.sink;
            Qt.callLater(root.rearmAudio);
        }
        function onVolumeChanged() {
            if (root.audioBaseline && root.previousSink === NacreAudio.sink)
                root.show();
        }
        function onMutedChanged() {
            if (root.audioBaseline && root.previousSink === NacreAudio.sink)
                root.show();
        }
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
        ignoreUnknownSignals: true
        function onAdjusted() {
            root.show("display");
        }
        function onExternalAdjusted() {
            if (!NacreDaemon.enabled)
                root.show("display");
        }
    }
    Connections {
        target: NacreKeyboardLight
        ignoreUnknownSignals: true
        function onAdjusted() {
            root.show("keyboard");
        }
        function onExternalAdjusted() {
            if (!NacreDaemon.enabled)
                root.show("keyboard");
        }
    }
    Connections {
        target: NacreCapsLock
        function onChanged(enabled) {
            root.show("capslock");
        }
    }
    Connections {
        target: PowerProfiles
        function onProfileChanged() {
            if (root.profileBaseline)
                root.show("power-profile");
        }
    }
    Connections {
        target: NacreControlTools
        function onDataChanged() {
            const next = NacreControlTools.data.nightLightEnabled === true;
            if (root.nightBaseline && root.nightReady && next !== root.lastNight)
                root.show("night-light");
            root.lastNight = next;
        }
    }
    Connections {
        target: NacreNotifs
        function onDndChanged() {
            root.show("dnd");
        }
    }
    Connections {
        target: NacreCompositor
        function onFocusedMonitorChanged() {
            if (root.screen.name !== NacrePanelState.activeName())
                root.cancel();
        }
    }
    Connections {
        target: root.visibilities
        function onOsdChanged() {
            if (root.visibilities.osd)
                root.cancel();
        }
        function onSessionChanged() {
            if (root.visibilities.session)
                root.cancel();
        }
    }
    Connections {
        target: WallpaperPlayback
        function onLockedChanged() {
            if (WallpaperPlayback.locked)
                root.cancel();
        }
    }
    Connections {
        target: NacrePanelState
        function onHiddenChanged() {
            if (NacrePanelState.hidden)
                root.cancel();
        }
    }
    NacreLevelNotice {
        id: notice
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
