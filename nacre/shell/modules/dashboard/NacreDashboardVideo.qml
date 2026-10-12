import QtQuick
import QtMultimedia

Item {
    id: root
    property string path: ""
    MediaPlayer {
        id: video
        objectName: "dashboardWallpaperVideo"
        autoPlay: true
        onMediaStatusChanged: if (mediaStatus === MediaPlayer.LoadedMedia)
            play()
        source: "file://" + root.path
        loops: MediaPlayer.Infinite
        videoOutput: videoSurface
    }
    VideoOutput {
        id: videoSurface
        anchors.fill: parent
        fillMode: VideoOutput.PreserveAspectCrop
    }
}
