pragma Singleton
import QtQuick
import qs.utils
import Quickshell
import Quickshell.Io
import qs.widgets
import "../modules/dashboard/media/media.js" as Model

Singleton {
    id: root
    readonly property var player: NacrePlayers.active
    readonly property bool shown: NacrePanelState.providerViews.some(view => (view.detached || NacrePanelState.hidden !== true) && (view.dashboard && view.dashboardTab === 2))
    readonly property bool spectrumVisible: NacrePanelState.providerViews.some(view => (view.detached || NacrePanelState.hidden !== true) && (view.mediaMini || view.dashboard && view.dashboardTab === 0))
    readonly property var lyricMetadata: Model.lyricsMetadata(player?.trackTitle, player?.trackArtist, player?.trackAlbum, /chrome|chromium|firefox|brave|vivaldi|browser/i.test((player?.desktopEntry || "") + " " + (player?.identity || "")))
    readonly property string trackKey: JSON.stringify({
        title: lyricMetadata.title,
        artist: lyricMetadata.artist,
        album: lyricMetadata.album,
        duration: player?.length || 0,
        art: "",
        lyrics: NacreDashboardState.preferences.lyrics !== false,
        palette: false,
        mode: "dark"
    })
    property string requestedKey: ""
    property string loadedKey: ""
    property var lines: []
    property string plain: ""
    property string message: ""
    property var spectrum: []
    readonly property string python: NacrePaths.palettePython
    readonly property string tool: NacrePaths.dashboardTools + "/dashboard-media.py"
    function load() {
        if (!shown || !player || details.running || loadedKey === trackKey)
            return;
        requestedKey = trackKey;
        details.command = [python, tool, requestedKey];
        details.running = true;
    }
    onTrackKeyChanged: {
        lines = [];
        plain = "";
        message = "";
        load();
    }
    onShownChanged: {
        if (shown)
            load();
        else
            loadedKey = "";
    }
    NacreHelperTask {
        id: details
        onOutput: text => {
            if (root.requestedKey !== root.trackKey)
                return;
            try {
                const data = JSON.parse(text);
                if (data.error)
                    throw new Error();
                root.lines = data.lines || [];
                root.plain = data.plain || "";
                root.message = data.lyricsMessage || "";
                root.loadedKey = root.requestedKey;
            } catch (error) {
                root.message = NacreI18n.tr("ui.media-details.message.media-details-unavailable", "Media details unavailable");
            }
        }
        onExited: if (root.requestedKey !== root.trackKey)
            Qt.callLater(root.load)
    }
    Process {
        id: visualizer
        command: ["python3", root.tool, "spectrum"]
        running: root.spectrumVisible && root.player?.isPlaying === true && NacreTokens.motionEnabled && !WallpaperPlayback.sleeping && !WallpaperPlayback.locked
        stdout: SplitParser {
            onRead: line => {
                const values = line.trim().split(";").filter(value => value !== "").map(value => Number(value));
                if (values.length === 32 && values.every(value => Number.isFinite(value) && value >= 0 && value <= 1000))
                    root.spectrum = values.map(value => value / 1000);
            }
        }
        onRunningChanged: if (!running)
            root.spectrum = []
    }
}
