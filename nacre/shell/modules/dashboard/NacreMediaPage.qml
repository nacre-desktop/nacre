import QtQuick
import QtQuick.Effects
import Quickshell
import Quickshell.Services.Mpris
import qs.widgets
import qs.services
import "media" as Controls
import "media/media.js" as Model

Item {
    id: root
    required property bool shouldUpdate
    required property PersistentProperties visibilities
    readonly property var player: NacrePlayers.active
    readonly property color pagePrimary: NacreTokens.accent
    readonly property color pageSecondary: NacreTokens.orientSecondary
    readonly property color pageInk: NacreTokens.ink
    readonly property var lyricLines: NacreMediaDetails.lines
    readonly property int lyricIndex: {
        let result = -1;
        for (let i = 0; i < lyricLines.length && lyricLines[i].time <= positionSeconds; i++)
            result = i;
        return result;
    }
    property real positionSeconds: 0
    property string artwork: ""
    property var seekOwner: null
    property var seekTrack: null
    property var volumeOwner: null
    readonly property bool sampling: clock.running
    readonly property bool canSeek: !!(shouldUpdate && player?.canControl && player?.canSeek && player?.positionSupported && player?.lengthSupported && Number.isFinite(player.length) && player.length > 0)
    readonly property real progress: player?.lengthSupported && player?.length > 0 ? Model.fraction(positionSeconds / player.length) : 0
    readonly property bool compact: width < 680
    readonly property real inset: width < 400 ? 14 : 22
    readonly property real secondaryHeight: Math.max(78, Math.min(150, height * .22))
    readonly property real playbackWidth: compact ? width - inset * 2 : Math.max(0, width * .62 - inset * 2)
    readonly property real playbackHeight: compact ? height - 66 - secondaryHeight - 8 : height - 82
    implicitWidth: 940
    implicitHeight: 562
    function member(actor) {
        return actor && [...NacrePlayers.list].includes(actor);
    }
    function refresh() {
        if (!seek.pressed)
            positionSeconds = player?.positionSupported ? player.position : 0;
    }
    function art() {
        if (shouldUpdate)
            artwork = player?.trackArtUrl || "";
    }
    function beginSeek() {
        seekOwner = player;
        seekTrack = player?.uniqueId;
    }
    function seekTo(fraction, actor, track) {
        if (!canSeek || !Number.isFinite(fraction) || actor !== player || !member(actor) || actor.uniqueId !== track)
            return;
        actor.position = Model.fraction(fraction) * actor.length;
        refresh();
    }
    function choose(actor) {
        if (shouldUpdate && member(actor))
            NacrePlayers.manualActive = actor;
    }
    function perform(action) {
        const actor = player;
        if (!shouldUpdate || !member(actor) || !actor.canControl)
            return;
        if (action === "previous" && actor.canGoPrevious)
            actor.previous();
        else if (action === "toggle" && actor.canTogglePlaying)
            actor.togglePlaying();
        else if (action === "next" && actor.canGoNext)
            actor.next();
        else if (action === "shuffle" && actor.shuffleSupported)
            actor.shuffle = !actor.shuffle;
        else if (action === "repeat" && actor.loopSupported) {
            const states = [MprisLoopState.None, MprisLoopState.Track, MprisLoopState.Playlist];
            actor.loopState = states[(states.indexOf(actor.loopState) + 1) % states.length];
        }
    }
    function volumeTo(value, actor) {
        if (shouldUpdate && actor === player && member(actor) && actor.canControl && actor.volumeSupported && Number.isFinite(value))
            actor.volume = Model.fraction(value);
    }
    function raisePlayer() {
        const actor = player;
        if (shouldUpdate && member(actor) && actor.canRaise) {
            actor.raise();
            visibilities.dashboard = false;
        }
    }
    onPlayerChanged: {
        refresh();
        if (!player)
            artwork = "";
        else
            art();
    }
    onShouldUpdateChanged: if (shouldUpdate) {
        refresh();
        art();
    }
    Component.onCompleted: {
        refresh();
        art();
    }
    Connections {
        target: root.player
        ignoreUnknownSignals: true
        function onTrackChanged() {
            root.positionSeconds = 0;
        }
        function onPostTrackChanged() {
            root.refresh();
            root.art();
        }
        function onPositionChanged() {
            root.refresh();
        }
        function onTrackArtUrlChanged() {
            root.art();
        }
    }
    Timer {
        id: clock
        interval: 1000
        repeat: true
        running: !!(root.shouldUpdate && root.visible && root.player?.isPlaying && root.player?.positionSupported && !seek.pressed)
        onTriggered: root.refresh()
    }
    NacreClip {
        id: hero
        objectName: "mediaHero"
        anchors.fill: parent
        radius: 20
        color: NacreTokens.raised
        // The diffuse backdrop needs fewer pixels than the crisp cover.
        Item {
            width: hero.width / 2
            height: hero.height / 2
            scale: 2
            transformOrigin: Item.TopLeft
            Image {
                id: backdrop
                anchors.fill: parent
                source: root.shouldUpdate ? root.artwork : ""
                sourceSize.width: 320
                sourceSize.height: 240
                fillMode: Image.PreserveAspectCrop
                asynchronous: true
                visible: false
            }
            MultiEffect {
                anchors.fill: parent
                source: backdrop
                blurEnabled: true
                blurMax: 32
                blur: .85
                opacity: .4
                visible: root.shouldUpdate && backdrop.status === Image.Ready
            }
        }
        Rectangle {
            anchors.fill: parent
            color: "transparent"
            gradient: Gradient {
                orientation: Gradient.Horizontal
                GradientStop {
                    position: 0
                    color: Qt.alpha(hero.color, .7)
                }
                GradientStop {
                    position: 1
                    color: Qt.alpha(hero.color, .8)
                }
            }
        }
        Rectangle {
            anchors.fill: parent
            color: "transparent"
            radius: 20
            border.width: 1
            border.color: Qt.alpha(root.pagePrimary, .25)
        }
        Flickable {
            id: choices
            objectName: "fullPlayerChoices"
            x: root.inset
            y: 16
            width: parent.width - root.inset * 2
            height: 38
            contentWidth: playerSegments.width
            contentHeight: height
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            flickableDirection: Flickable.HorizontalFlick
            Row {
                id: playerSegments
                spacing: 4
                Repeater {
                    model: NacrePlayers.list
                    delegate: NacreSurface {
                        id: segment
                        required property var modelData
                        readonly property bool selected: modelData === root.player
                        width: Math.min(180, segmentLabel.implicitWidth + 54)
                        height: 36
                        radius: 18
                        color: selected ? Qt.alpha(root.pagePrimary, .16) : Qt.alpha(hero.color, .6)
                        Row {
                            anchors.centerIn: parent
                            spacing: 7
                            Image {
                                width: 18
                                height: 18
                                source: Quickshell.iconPath(segment.modelData.desktopEntry || "multimedia-player", true)
                                sourceSize.width: 18
                                sourceSize.height: 18
                                anchors.verticalCenter: parent.verticalCenter
                                visible: status === Image.Ready
                            }
                            NacreText {
                                id: segmentLabel
                                width: Math.min(126, implicitWidth)
                                text: segment.modelData.identity || NacreI18n.tr("dashboard.media-page.text.media-player", "Media player")
                                font.pointSize: 10
                                font.weight: segment.selected ? Font.DemiBold : Font.Normal
                                color: root.pageInk
                                elide: Text.ElideRight
                            }
                        }
                        NacreInteraction {
                            accessibleName: segmentLabel.text
                            function onClicked() {
                                root.choose(segment.modelData);
                            }
                        }
                    }
                }
            }
        }
        Item {
            id: playback
            objectName: "mediaPlayback"
            x: root.inset
            y: 60
            width: root.playbackWidth
            height: root.playbackHeight
            Item {
                id: metadata
                width: parent.width
                height: Math.max(72, Math.min(root.compact ? 144 : 260, parent.height - transport.height - 20))
                NacreClip {
                    id: cover
                    objectName: "fullAlbum"
                    width: Math.min(parent.height, parent.width * (root.compact ? .32 : .4))
                    height: width
                    anchors.verticalCenter: parent.verticalCenter
                    radius: 14
                    color: Qt.alpha(root.pagePrimary, .12)
                    Image {
                        id: coverArt
                        objectName: "fullAlbumImage"
                        anchors.fill: parent
                        source: root.artwork
                        sourceSize.width: 480
                        sourceSize.height: 480
                        asynchronous: true
                        retainWhileLoading: true
                        fillMode: Image.PreserveAspectCrop
                    }
                    NacreIcon {
                        anchors.centerIn: parent
                        text: "music_note"
                        font.pointSize: Math.max(24, cover.width * .22)
                        color: root.pagePrimary
                        visible: coverArt.status !== Image.Ready && coverArt.status !== Image.Loading
                    }
                }
                Column {
                    x: cover.width + (root.compact ? 14 : 22)
                    width: parent.width - x
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: root.compact ? 7 : 12
                    NacreText {
                        objectName: "fullMediaTitle"
                        width: parent.width
                        text: root.player ? root.player.trackTitle || NacreI18n.tr("dashboard.media-page.text.unknown-track", "Unknown track") : NacreI18n.tr("dashboard.media-page.text.no-media-playing", "No media playing")
                        color: root.pageInk
                        font.pointSize: root.compact ? 18 : 23
                        font.weight: Font.DemiBold
                        wrapMode: Text.Wrap
                        maximumLineCount: root.compact ? 2 : 3
                        elide: Text.ElideRight
                    }
                    NacreText {
                        width: parent.width
                        text: root.player ? root.player.trackArtist || root.player.identity : NacreI18n.tr("dashboard.media-page.text.open-a-music-or-video-app", "Open a music or video app")
                        color: root.pageSecondary
                        font.pointSize: root.compact ? 11 : 13
                        wrapMode: Text.Wrap
                        maximumLineCount: 2
                        elide: Text.ElideRight
                    }
                    NacreText {
                        width: parent.width
                        text: root.player?.trackAlbum || ""
                        visible: text.length > 0
                        color: Qt.alpha(root.pageInk, .65)
                        font.pointSize: 10
                        elide: Text.ElideRight
                    }
                }
            }
            Column {
                id: transport
                objectName: "mediaTransport"
                anchors.bottom: parent.bottom
                width: parent.width
                spacing: root.compact ? 2 : 8
                Controls.NacreMediaSlider {
                    id: seek
                    height: root.compact ? 24 : 34
                    accent: root.pagePrimary
                    second: root.pageSecondary
                    track: Qt.alpha(root.pageInk, .14)
                    objectName: "fullSeek"
                    width: parent.width
                    enabled: root.canSeek
                    label: NacreI18n.tr("dashboard.media-page.label.track-position", "Track position")
                    progress: root.progress
                    onDragBegan: root.beginSeek()
                    onDragEnded: fraction => root.seekTo(fraction, root.seekOwner, root.seekTrack)
                    onKeyboardRequested: fraction => root.seekTo(fraction, root.player, root.player?.uniqueId)
                }
                Item {
                    width: parent.width
                    height: root.compact ? 14 : 18
                    NacreText {
                        text: root.player?.positionSupported ? Model.duration(root.positionSeconds) : "—"
                        font.pointSize: 9
                        color: Qt.alpha(root.pageInk, .65)
                    }
                    NacreText {
                        anchors.right: parent.right
                        text: root.player?.lengthSupported ? Model.duration(root.player.length) : "—"
                        font.pointSize: 9
                        color: Qt.alpha(root.pageInk, .65)
                    }
                }
                Row {
                    anchors.horizontalCenter: parent.horizontalCenter
                    spacing: root.compact ? 6 : 12
                    Controls.NacreMediaButton {
                        accent: root.pagePrimary
                        ink: root.pageInk
                        anchors.verticalCenter: parent.verticalCenter
                        icon: "shuffle"
                        label: NacreI18n.tr("dashboard.media-page.label.shuffle", "Shuffle")
                        selected: root.player?.shuffle === true
                        available: !!(root.shouldUpdate && root.player?.canControl && root.player?.shuffleSupported)
                        onClicked: root.perform("shuffle")
                    }
                    Controls.NacreMediaButton {
                        accent: root.pagePrimary
                        ink: root.pageInk
                        objectName: "fullPrevious"
                        anchors.verticalCenter: parent.verticalCenter
                        icon: "skip_previous"
                        label: NacreI18n.tr("dashboard.media-page.label.previous-track", "Previous track")
                        available: !!(root.shouldUpdate && root.player?.canControl && root.player?.canGoPrevious)
                        onClicked: root.perform("previous")
                    }
                    Controls.NacreMediaButton {
                        accent: root.pagePrimary
                        ink: root.pageInk
                        objectName: "fullToggle"
                        width: root.compact ? 40 : 56
                        height: width
                        selected: true
                        icon: root.player?.isPlaying ? "pause" : "play_arrow"
                        label: root.player?.isPlaying ? NacreI18n.tr("dashboard.media-page.label.pause", "Pause") : NacreI18n.tr("dashboard.media-page.label.play", "Play")
                        available: !!(root.shouldUpdate && root.player?.canControl && root.player?.canTogglePlaying)
                        onClicked: root.perform("toggle")
                    }
                    Controls.NacreMediaButton {
                        accent: root.pagePrimary
                        ink: root.pageInk
                        objectName: "fullNext"
                        anchors.verticalCenter: parent.verticalCenter
                        icon: "skip_next"
                        label: NacreI18n.tr("dashboard.media-page.label.next-track", "Next track")
                        available: !!(root.shouldUpdate && root.player?.canControl && root.player?.canGoNext)
                        onClicked: root.perform("next")
                    }
                    Controls.NacreMediaButton {
                        accent: root.pagePrimary
                        ink: root.pageInk
                        anchors.verticalCenter: parent.verticalCenter
                        icon: root.player?.loopState === MprisLoopState.Track ? "repeat_one" : "repeat"
                        label: NacreI18n.tr("dashboard.media-page.label.cycle-repeat-mode", "Cycle repeat mode")
                        selected: !!root.player?.loopState
                        available: !!(root.shouldUpdate && root.player?.canControl && root.player?.loopSupported)
                        onClicked: root.perform("repeat")
                    }
                }
                Row {
                    width: parent.width
                    spacing: 8
                    visible: root.player?.volumeSupported === true
                    NacreIcon {
                        text: "volume_up"
                        color: root.pageInk
                        anchors.verticalCenter: parent.verticalCenter
                        font.pointSize: 13
                    }
                    Controls.NacreMediaSlider {
                        accent: root.pagePrimary
                        second: root.pageSecondary
                        track: Qt.alpha(root.pageInk, .14)
                        objectName: "fullVolume"
                        height: root.compact ? 24 : 34
                        width: parent.width - 68
                        label: NacreI18n.tr("dashboard.media-page.label.player-volume", "Player volume")
                        progress: Model.fraction(root.player?.volume)
                        enabled: !!(root.shouldUpdate && root.player?.canControl)
                        onDragBegan: root.volumeOwner = root.player
                        onDragEnded: value => root.volumeTo(value, root.volumeOwner)
                        onKeyboardRequested: value => root.volumeTo(value, root.player)
                    }
                    NacreText {
                        text: NacreI18n.tr("dashboard.media-page.text.volume-percent", "{value}%", {
                            value: Math.round(Model.fraction(root.player?.volume) * 100)
                        })
                        color: Qt.alpha(root.pageInk, .65)
                        font.pointSize: 9
                        anchors.verticalCenter: parent.verticalCenter
                    }
                }
            }
        }
        NacreSurface {
            id: secondary
            objectName: "mediaSecondary"
            x: root.compact ? root.inset : playback.x + playback.width + 22
            y: root.compact ? root.height - root.secondaryHeight - 16 : playback.y
            width: root.compact ? root.width - root.inset * 2 : root.width - x - root.inset
            height: root.compact ? root.secondaryHeight : playback.height
            radius: 16
            color: Qt.alpha(hero.color, .6)
            NacreText {
                id: secondaryTitle
                visible: !root.compact || !root.lyricLines.length
                x: 16
                y: 12
                width: parent.width - 32
                text: (root.lyricLines.length || NacreMediaDetails.plain) ? NacreI18n.tr("dashboard.media-page.text.lyrics", "Lyrics") : NacreI18n.tr("dashboard.media-page.text.track-details", "Track details")
                color: Qt.alpha(root.pageInk, .65)
                font.pointSize: 10
            }
            ListView {
                id: lyrics
                objectName: "mediaLyrics"
                x: 16
                y: root.compact ? 12 : 38
                width: parent.width - 32
                height: parent.height - y - (root.compact ? 12 : 36)
                visible: root.lyricLines.length > 0
                clip: true
                model: root.lyricLines
                currentIndex: Math.max(0, root.lyricIndex)
                highlightRangeMode: ListView.StrictlyEnforceRange
                preferredHighlightBegin: height * .35
                preferredHighlightEnd: height * .35
                highlightMoveDuration: NacreTokens.motionEnabled ? 350 : 0
                spacing: root.compact ? 8 : 20
                delegate: NacreText {
                    required property var modelData
                    required property int index
                    width: lyrics.width
                    text: modelData.text
                    wrapMode: Text.Wrap
                    font.pointSize: root.compact ? 12 : 17
                    font.weight: index === root.lyricIndex ? Font.DemiBold : Font.Normal
                    color: index === root.lyricIndex ? root.pagePrimary : Qt.alpha(root.pageInk, .45)
                    Behavior on color {
                        ColorAnimation {
                            duration: NacreTokens.motionEnabled ? 200 : 0
                        }
                    }
                }
            }
            Flickable {
                id: plainLyrics
                objectName: "mediaPlainLyrics"
                x: lyrics.x
                y: lyrics.y
                width: lyrics.width
                height: lyrics.height
                clip: true
                visible: !root.lyricLines.length && NacreMediaDetails.plain.length > 0
                contentHeight: plainText.height
                boundsBehavior: Flickable.StopAtBounds
                NacreText {
                    id: plainText
                    width: parent.width
                    text: NacreMediaDetails.plain
                    wrapMode: Text.Wrap
                    color: root.pageInk
                    font.pointSize: root.compact ? 12 : 15
                    lineHeight: 1.35
                }
            }
            Column {
                objectName: "mediaTrackDetails"
                x: 16
                y: 40
                width: parent.width - 32
                spacing: root.compact ? 4 : 22
                visible: !lyrics.visible && !plainLyrics.visible
                NacreText {
                    width: parent.width
                    text: root.player?.trackAlbum || root.player?.trackTitle || NacreI18n.tr("dashboard.media-page.text.ready-when-you-are", "Ready when you are")
                    font.pointSize: root.compact ? 12 : 19
                    font.weight: Font.DemiBold
                    color: root.pageInk
                    wrapMode: Text.Wrap
                    maximumLineCount: root.compact ? 1 : 3
                    elide: Text.ElideRight
                }
                NacreText {
                    width: parent.width
                    text: root.player ? root.player.trackArtist || root.player.identity : NacreI18n.tr("dashboard.media-page.text.empty-details", "Play a track to see artwork, playback controls and lyrics here.")
                    font.pointSize: 10
                    color: Qt.alpha(root.pageInk, .65)
                    wrapMode: Text.Wrap
                    maximumLineCount: root.compact ? 2 : 4
                    elide: Text.ElideRight
                }
                NacreText {
                    width: parent.width
                    text: root.player?.lengthSupported ? NacreI18n.tr("dashboard.media-page.text.duration", "Duration  {value}", {
                        value: Model.duration(root.player.length)
                    }) : ""
                    font.pointSize: 10
                    color: Qt.alpha(root.pageInk, .65)
                    visible: !root.compact && text.length > 0
                }
                ActionButton {
                    objectName: "fullRaise"
                    width: Math.min(implicitWidth, parent.width)
                    text: root.player?.identity || NacreI18n.tr("dashboard.media-page.text.player", "Player")
                    icon: "open_in_new"
                    compact: true
                    visible: !root.compact && !!root.player
                    enabled: !!(root.shouldUpdate && root.player?.canRaise)
                    onClicked: root.raisePlayer()
                }
            }
            NacreText {
                x: 16
                width: parent.width - 32
                anchors.bottom: parent.bottom
                anchors.bottomMargin: 10
                text: root.lyricLines.length || NacreMediaDetails.plain ? NacreI18n.tr("dashboard.media-page.text.lyrics-lrclib", "Lyrics: LRCLIB") : ""
                visible: !root.compact
                color: Qt.alpha(root.pageInk, .5)
                font.pointSize: 8
            }
        }
    }
}
