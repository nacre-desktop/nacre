import QtQuick
import QtQuick.Effects
import QtQuick.Layouts
import qs.widgets
import qs.services

NacreClip {
    id: root
    property bool active: false
    readonly property var player: NacrePlayers.active
    width: parent.width
    height: 76
    radius: 22
    color: NacreTokens.raised
    clip: true
    Image {
        id: wash
        anchors.fill: parent
        source: root.active ? root.player?.trackArtUrl || "" : ""
        sourceSize: Qt.size(240, 80)
        asynchronous: true
        fillMode: Image.PreserveAspectCrop
        visible: false
    }
    MultiEffect {
        anchors.fill: wash
        source: wash
        blurEnabled: true
        blur: .65
        blurMax: 24
        autoPaddingEnabled: false
        opacity: NacreTokens.light ? .14 : .2
        visible: root.active && wash.status === Image.Ready
    }
    Rectangle {
        anchors.fill: parent
        color: Qt.alpha(NacreTokens.body, NacreTokens.light ? .2 : .4)
    }
    Image {
        x: 12
        y: 16
        width: 44
        height: 44
        source: root.active ? root.player?.trackArtUrl || "" : ""
        sourceSize: Qt.size(88, 88)
        asynchronous: true
        fillMode: Image.PreserveAspectCrop
    }
    Column {
        x: 68
        anchors.verticalCenter: parent.verticalCenter
        width: Math.max(0, parent.width - x - 118)
        spacing: 4
        NacreText {
            width: parent.width
            text: root.player?.trackTitle || NacreI18n.tr("controlcenter.overview.now-playing", "Now playing")
            font.pointSize: 10
            font.weight: Font.Medium
            elide: Text.ElideRight
        }
        NacreText {
            width: parent.width
            text: root.player?.trackArtist || root.player?.identity || ""
            font.pointSize: 9
            color: NacreTokens.mutedInk
            elide: Text.ElideRight
        }
    }
    Row {
        anchors.right: parent.right
        anchors.rightMargin: 8
        anchors.verticalCenter: parent.verticalCenter
        spacing: 2
        NacreControlButton {
            icon: "skip_previous"
            label: NacreI18n.tr("controlcenter.media.previous", "Previous track")
            enabled: root.active && root.player?.canGoPrevious === true
            onClicked: NacrePlayers.control("previous")
        }
        NacreControlButton {
            icon: root.player?.isPlaying ? "pause" : "play_arrow"
            label: root.player?.isPlaying ? NacreI18n.tr("controlcenter.media.pause", "Pause playback") : NacreI18n.tr("controlcenter.media.play", "Play media")
            enabled: root.active && root.player?.canTogglePlaying === true
            onClicked: NacrePlayers.control("playPause")
        }
        NacreControlButton {
            icon: "skip_next"
            label: NacreI18n.tr("controlcenter.media.next", "Next track")
            enabled: root.active && root.player?.canGoNext === true
            onClicked: NacrePlayers.control("next")
        }
    }
}
