import QtQuick
import QtMultimedia

Item {
    id: root
    property string path: ""
    VideoOutput {
        id: output
        anchors.fill: parent
        fillMode: VideoOutput.PreserveAspectCrop
    }
    MediaPlayer {
        source: root.path ? "file://" + encodeURIComponent(root.path).replace(/%2F/g, "/") : ""
        videoOutput: output
        audioOutput: null
        activeAudioTrack: -1
        loops: MediaPlayer.Infinite
        Component.onCompleted: play()
        onMediaStatusChanged: if (mediaStatus === MediaPlayer.LoadedMedia)
            play()
    }
}
