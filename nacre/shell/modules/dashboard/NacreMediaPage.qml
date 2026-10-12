import QtQuick
import QtQuick.Controls.Basic
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
    readonly property color pagePrimary: NacreMediaDetails.palette.primary || NacreTokens.accent
    readonly property color pageSecondary: NacreMediaDetails.palette.secondary || NacreTokens.orientSecondary
    readonly property color pageInk: NacreMediaDetails.palette.onSurface || NacreTokens.ink
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
    readonly property bool compact: width < 620
    implicitWidth: 766
    implicitHeight: Math.max(compact ? 800 : 540, choices.visible ? choices.y + choices.height + 16 : details.y + details.height + 16)
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
    Rectangle {
        anchors.fill: parent
        radius: 20
        border.width: 1
        border.color: Qt.alpha(root.pagePrimary, .3)
        color: NacreMediaDetails.palette.surface || NacreTokens.raised
    }
    Flickable {
        id: scroll
        anchors.fill: parent
        contentWidth: width
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: NacreScrollBar {}
        FastScroll {
            view: scroll
        }
        Controls.NacreAlbum {
            id: album
            objectName: "fullAlbum"
            x: root.compact ? (root.width - width) / 2 : 8
            y: root.compact ? 10 : 42
            width: root.compact ? Math.min(206, root.width - 32) : 224
            height: width
            source: root.artwork
            progress: root.progress
            first: root.pagePrimary
            second: root.pageSecondary
            spectrum: root.shouldUpdate ? NacreMediaDetails.spectrum : []
        }
        Column {
            id: details
            x: root.compact ? 16 : 254
            y: root.compact ? album.y + album.height + 14 : 12
            width: Math.max(0, root.width - x - (root.compact ? 16 : 12))
            spacing: 9
            NacreText {
                objectName: "fullMediaTitle"
                width: parent.width
                text: root.player ? root.player.trackTitle || NacreI18n.tr("dashboard.media-page.text.unknown-track", "Unknown track") : NacreI18n.tr("dashboard.media-page.text.no-media-playing", "No media playing")
                color: root.pagePrimary
                font.pointSize: 17
                elide: Text.ElideRight
                horizontalAlignment: Text.AlignHCenter
            }
            NacreText {
                width: parent.width
                text: root.player?.trackAlbum || ""
                visible: text.length > 0
                color: Qt.alpha(root.pageInk, .72)
                font.pointSize: 11
                elide: Text.ElideRight
                horizontalAlignment: Text.AlignHCenter
            }
            NacreText {
                width: parent.width
                text: root.player ? root.player.trackArtist || root.player.identity : NacreI18n.tr("dashboard.media-page.text.open-a-music-or-video-app", "Open a music or video app")
                color: root.pageSecondary
                font.pointSize: 12
                elide: Text.ElideRight
                horizontalAlignment: Text.AlignHCenter
            }
            Row {
                anchors.horizontalCenter: parent.horizontalCenter
                spacing: 8
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
                    width: 48
                    height: 48
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
            }
            Controls.NacreMediaSlider {
                id: seek
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
                height: 18
                NacreText {
                    text: root.player?.positionSupported ? Model.duration(root.positionSeconds) : "—"
                    font.pointSize: 10
                    color: Qt.alpha(root.pageInk, .72)
                }
                NacreText {
                    anchors.right: parent.right
                    text: root.player?.lengthSupported ? Model.duration(root.player.length) : "—"
                    font.pointSize: 10
                    color: Qt.alpha(root.pageInk, .72)
                }
            }
            Row {
                anchors.horizontalCenter: parent.horizontalCenter
                spacing: 6
                Controls.NacreMediaButton {
                    accent: root.pagePrimary
                    ink: root.pageInk
                    icon: "shuffle"
                    label: NacreI18n.tr("dashboard.media-page.label.shuffle", "Shuffle")
                    visible: root.player?.shuffleSupported === true
                    selected: root.player?.shuffle === true
                    available: !!(root.shouldUpdate && root.player?.canControl)
                    onClicked: root.perform("shuffle")
                }
                Controls.NacreMediaButton {
                    accent: root.pagePrimary
                    ink: root.pageInk
                    icon: root.player?.loopState === MprisLoopState.Track ? "repeat_one" : "repeat"
                    label: NacreI18n.tr("dashboard.media-page.label.cycle-repeat-mode", "Cycle repeat mode")
                    visible: root.player?.loopSupported === true
                    selected: !!root.player?.loopState
                    available: !!(root.shouldUpdate && root.player?.canControl)
                    onClicked: root.perform("repeat")
                }
                ActionButton {
                    objectName: "fullRaise"
                    text: root.player?.identity || NacreI18n.tr("dashboard.media-page.text.player", "Player")
                    icon: "open_in_new"
                    compact: true
                    enabled: !!(root.shouldUpdate && root.player?.canRaise)
                    onClicked: root.raisePlayer()
                }
            }
            Row {
                width: parent.width
                spacing: 8
                visible: root.player?.volumeSupported === true
                NacreIcon {
                    text: "volume_up"
                    anchors.verticalCenter: parent.verticalCenter
                }
                Controls.NacreMediaSlider {
                    accent: root.pagePrimary
                    second: root.pageSecondary
                    track: Qt.alpha(root.pageInk, .14)
                    objectName: "fullVolume"
                    width: parent.width - 36
                    label: NacreI18n.tr("dashboard.media-page.label.player-volume", "Player volume")
                    progress: Model.fraction(root.player?.volume)
                    enabled: !!(root.shouldUpdate && root.player?.canControl)
                    onDragBegan: root.volumeOwner = root.player
                    onDragEnded: value => root.volumeTo(value, root.volumeOwner)
                    onKeyboardRequested: value => root.volumeTo(value, root.player)
                }
            }
        }
        Column {
            id: lyrics
            x: root.compact ? 16 : 254
            y: details.y + details.height + 20
            width: Math.max(0, root.width - x - 16)
            spacing: 10
            NacreText {
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                text: root.lyricIndex > 0 ? root.lyricLines[root.lyricIndex - 1].text : ""
                font.pointSize: 10
                color: root.pageInk
                opacity: .55
                elide: Text.ElideRight
            }
            NacreText {
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                text: root.lyricIndex >= 0 ? root.lyricLines[root.lyricIndex].text : NacreMediaDetails.message || (root.player ? NacreI18n.tr("dashboard.media-page.text.loading-track-details", "Loading track details…") : "")
                font.pointSize: 15
                color: root.pagePrimary
                wrapMode: Text.Wrap
            }
            NacreText {
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                text: root.lyricIndex >= 0 && root.lyricIndex + 1 < root.lyricLines.length ? root.lyricLines[root.lyricIndex + 1].text : ""
                font.pointSize: 10
                color: root.pageInk
                opacity: .55
                elide: Text.ElideRight
            }
            Row {
                anchors.horizontalCenter: parent.horizontalCenter
                spacing: 8
                ActionButton {
                    text: NacreI18n.tr("dashboard.media-page.text.output-device", "Output device")
                    icon: "speaker"
                    compact: true
                    onClicked: NacrePanelState.openControls("audio")
                }
                ActionButton {
                    text: NacreI18n.tr("dashboard.media-page.text.album-colors", "Album colors")
                    compact: true
                    selected: NacreDashboardState.preferences.albumPalette !== false
                    onClicked: NacreDashboardState.save("albumPalette", !NacreDashboardState.preferences.albumPalette)
                }
            }
            NacreText {
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                text: NacreMediaDetails.lines.length ? NacreI18n.tr("dashboard.media-page.text.lyrics-lrclib", "Lyrics: LRCLIB") : ""
                font.pointSize: 8
                color: root.pageInk
                opacity: .6
            }
        }
        Flow {
            id: choices
            objectName: "fullPlayerChoices"
            visible: NacrePlayers.list.length > 1
            x: 16
            y: lyrics.y + lyrics.height + 12
            width: parent.width - 32
            spacing: 6
            Repeater {
                model: NacrePlayers.list
                delegate: ActionButton {
                    required property var modelData
                    text: modelData.identity || NacreI18n.tr("dashboard.media-page.text.media-player", "Media player")
                    compact: true
                    selected: modelData === root.player
                    width: Math.min(implicitWidth, choices.width)
                    clip: true
                    onClicked: root.choose(modelData)
                }
            }
        }
        contentHeight: Math.max(root.implicitHeight, lyrics.y + lyrics.height + 16, choices.visible ? choices.y + choices.height + 16 : 0)
    }
}
