import QtQuick
import QtQuick.Shapes
import qs.widgets
import qs.services
import "overview.js" as Overview
import ".." as Dashboard

NacreOverviewCard {
    id: root
    property bool active: false
    property bool compact: false
    property bool audioHalo: false
    property var spectrum: []
    readonly property var player: NacrePlayers.active
    property real positionSeconds: 0
    property bool artReady: false
    property string artUrl: ""
    readonly property bool sampling: sample.running
    readonly property real progress: player?.positionSupported && player?.lengthSupported ? Overview.progress(positionSeconds, player.length) : 0
    readonly property real artSize: Math.max(40, Math.min(width - 48, compact ? Math.min(200, Math.max(132, width * .36)) : 174, height - (audioHalo ? 180 : compact ? 136 : 190)))
    function refresh() {
        positionSeconds = player?.positionSupported ? player.position : 0;
    }
    function refreshArt() {
        if (active)
            artUrl = player?.trackArtUrl || "";
    }
    function perform(action) {
        const current = player;
        if (!active || !current?.canControl)
            return;
        if (action === "previous" && current.canGoPrevious)
            current.previous();
        else if (action === "toggle" && current.canTogglePlaying)
            current.togglePlaying();
        else if (action === "next" && current.canGoNext)
            current.next();
    }
    onPlayerChanged: {
        refresh();
        if (!player)
            artUrl = "";
        else
            refreshArt();
    }
    onActiveChanged: if (active) {
        refresh();
        refreshArt();
    }
    Component.onCompleted: {
        refresh();
        refreshArt();
    }
    Connections {
        target: root.player
        ignoreUnknownSignals: true
        function onTrackChanged() {
            root.positionSeconds = 0;
        }
        function onPostTrackChanged() {
            root.refresh();
            root.refreshArt();
        }
        function onTrackArtUrlChanged() {
            root.refreshArt();
        }
        function onPositionChanged() {
            root.refresh();
        }
    }
    Timer {
        id: sample
        interval: 1000
        running: !!(root.active && root.visible && root.player?.isPlaying && root.player?.positionSupported && root.player?.lengthSupported)
        repeat: true
        onTriggered: root.refresh()
    }
    Item {
        id: art
        y: root.audioHalo ? 8 : root.compact ? 12 : 20
        width: root.artSize + (root.audioHalo ? 56 : 12)
        height: width
        anchors.horizontalCenter: parent.horizontalCenter
        NacreClip {
            id: album
            anchors.centerIn: parent
            width: root.artSize
            height: width
            radius: width / 2
            color: NacreColours.palette.m3surfaceContainerHigh
            Image {
                id: cover
                objectName: "mediaCover"
                anchors.fill: parent
                source: root.artUrl
                asynchronous: true
                cache: true
                retainWhileLoading: true
                sourceSize.width: 320
                sourceSize.height: 320
                fillMode: Image.PreserveAspectCrop
                visible: status === Image.Ready || status === Image.Loading && root.artReady
                onStatusChanged: {
                    if (status === Image.Ready)
                        root.artReady = true;
                    else if (status === Image.Error || status === Image.Null)
                        root.artReady = false;
                }
            }
            NacreIcon {
                anchors.centerIn: parent
                visible: !cover.visible
                text: "music_note"
                color: NacreColours.palette.m3onSurfaceVariant
                font.pointSize: 36
            }
        }
        Shape {
            id: progressRing
            anchors.centerIn: parent
            width: root.artSize + 12
            height: width
            preferredRendererType: Shape.CurveRenderer
            ShapePath {
                strokeGradient: LinearGradient {
                    x1: 0
                    y1: progressRing.height
                    x2: progressRing.width
                    y2: 0
                    GradientStop {
                        position: 0
                        color: NacreTokens.accent
                    }
                    GradientStop {
                        position: .5
                        color: NacreTokens.orientSecondary
                    }
                    GradientStop {
                        position: 1
                        color: NacreColours.palette.m3tertiary
                    }
                }
                strokeWidth: 5
                fillColor: "transparent"
                capStyle: ShapePath.RoundCap
                PathAngleArc {
                    centerX: progressRing.width / 2
                    centerY: progressRing.height / 2
                    radiusX: progressRing.width / 2 - 3
                    radiusY: radiusX
                    startAngle: -90
                    sweepAngle: root.progress * 360
                }
            }
        }
    }
    Dashboard.NacreAlbumHalo {
        anchors.fill: art
        coverSize: root.artSize
        samples: root.spectrum
        active: root.audioHalo && root.active && root.player?.isPlaying === true && NacreTokens.motionEnabled
    }
    Column {
        id: metadata
        x: 14
        y: art.y + art.height + (root.compact ? 8 : 14)
        width: root.width - 28
        spacing: root.compact ? 4 : 8
        NacreText {
            objectName: "mediaTitle"
            width: parent.width
            text: root.player ? root.player.trackTitle || NacreI18n.tr("overview.media-card.text.unknown-track", "Unknown track") : NacreI18n.tr("overview.media-card.text.no-media-playing", "No media playing")
            font.pointSize: 12
            color: NacreColours.palette.m3primary
            elide: Text.ElideRight
            horizontalAlignment: Text.AlignHCenter
        }
        NacreText {
            width: parent.width
            text: root.player?.trackAlbum || ""
            visible: !root.compact && text.length > 0
            color: NacreColours.palette.m3onSurfaceVariant
            font.pointSize: 10
            elide: Text.ElideRight
            horizontalAlignment: Text.AlignHCenter
        }
        NacreText {
            width: parent.width
            text: root.player ? root.player.trackArtist || root.player.identity : NacreI18n.tr("overview.media-card.text.open-a-music-or-video-app", "Open a music or video app")
            color: NacreColours.palette.m3secondary
            font.pointSize: 10
            fontSizeMode: root.player ? Text.FixedSize : Text.HorizontalFit
            minimumPointSize: 8
            elide: Text.ElideRight
            horizontalAlignment: Text.AlignHCenter
        }
    }
    Row {
        id: controls
        y: Math.min(root.height - 48, metadata.y + metadata.height + 16)
        anchors.horizontalCenter: parent.horizontalCenter
        spacing: 6
        Transport {
            objectName: "mediaPrevious"
            icon: "skip_previous"
            label: NacreI18n.tr("overview.media-card.label.previous-track", "Previous track")
            available: !!(root.active && root.player?.canControl && root.player?.canGoPrevious)
            onActivated: root.perform("previous")
        }
        Transport {
            objectName: "mediaToggle"
            icon: root.player?.isPlaying ? "pause" : "play_arrow"
            label: root.player?.isPlaying ? NacreI18n.tr("overview.media-card.label.pause", "Pause") : NacreI18n.tr("overview.media-card.label.play", "Play")
            available: !!(root.active && root.player?.canControl && root.player?.canTogglePlaying)
            onActivated: root.perform("toggle")
        }
        Transport {
            objectName: "mediaNext"
            icon: "skip_next"
            label: NacreI18n.tr("overview.media-card.label.next-track", "Next track")
            available: !!(root.active && root.player?.canControl && root.player?.canGoNext)
            onActivated: root.perform("next")
        }
    }
    Rectangle {
        visible: !root.compact
        x: 20
        y: parent.height - 18
        width: Math.max(0, parent.width - 40)
        height: 2
        radius: 1
        color: NacreColours.palette.m3outlineVariant
        Rectangle {
            width: root.progress * parent.width
            height: 2
            radius: 1
            color: NacreColours.palette.m3primary
        }
    }
    component Transport: NacreSurface {
        required property string icon
        required property string label
        property bool available: false
        signal activated
        width: 34
        height: 34
        radius: 17
        color: "transparent"
        opacity: available ? 1 : 0.35
        NacreIcon {
            anchors.centerIn: parent
            text: parent.icon
            font.pointSize: 17
        }
        NacreInteraction {
            disabled: !parent.available
            accessibleName: parent.label
            function onClicked() {
                parent.activated();
            }
        }
    }
}
