import QtQuick
import QtMultimedia
import Quickshell.Io

Item {
    id: root
    property string screenName: ""
    property string path: ""
    property bool running: false
    property string failure: ""
    property bool frameReady: false
    property int resumePosition: 0
    readonly property int position: player.position
    readonly property bool playing: player.playing
    readonly property bool audioDisabled: player.audioOutput === null && player.activeAudioTrack === -1
    function fileUrl(path) {
        return path ? "file://" + encodeURIComponent(path).replace(/%2F/g, "/") : "";
    }
    function syncPlayback() {
        if (!player)
            return;
        if (running && path)
            player.play();
        else
            player.pause();
    }
    function captureState() {
        return {
            path: path,
            position: player.position
        };
    }
    function restoreState(state) {
        if (state?.path !== path || !Number.isFinite(state.position))
            return;
        resumePosition = Math.max(0, Math.floor(state.position));
        if (player.mediaStatus === MediaPlayer.LoadedMedia || player.mediaStatus === MediaPlayer.BufferedMedia)
            restorePosition();
    }
    function restorePosition() {
        if (resumePosition > 0 && player.seekable) {
            player.position = Math.min(resumePosition, Math.max(0, player.duration - 1));
            resumePosition = 0;
        }
    }
    onRunningChanged: syncPlayback()
    onPathChanged: {
        frameReady = false;
        resumePosition = 0;
        failure = "";
        if (output)
            output.clearOutput();
        Qt.callLater(syncPlayback);
    }
    Component.onCompleted: syncPlayback()
    VideoOutput {
        id: output
        anchors.fill: parent
        fillMode: VideoOutput.PreserveAspectCrop
        endOfStreamPolicy: VideoOutput.KeepLastFrame
        visible: root.frameReady
    }
    Connections {
        target: output.videoSink
        function onVideoFrameChanged() {
            if (player.hasVideo && output.videoSink.videoSize.width > 0)
                root.frameReady = true;
        }
    }
    MediaPlayer {
        id: player
        source: root.fileUrl(root.path)
        videoOutput: output
        audioOutput: null
        activeAudioTrack: -1
        loops: MediaPlayer.Infinite
        onMediaStatusChanged: if (mediaStatus === MediaPlayer.LoadedMedia || mediaStatus === MediaPlayer.BufferedMedia)
            root.restorePosition()
        onErrorOccurred: (error, message) => {
            root.failure = message;
            root.frameReady = false;
        }
    }
    IpcHandler {
        target: "wallpaperMotion-" + root.screenName
        function state(): string {
            return JSON.stringify({
                path: root.path,
                running: root.running,
                playing: root.playing,
                position: root.position,
                error: root.failure,
                failure: root.failure,
                firstFrame: root.frameReady,
                audioDisabled: root.audioDisabled
            });
        }
    }
}
