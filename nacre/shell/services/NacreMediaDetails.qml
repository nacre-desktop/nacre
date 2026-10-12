pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io
import qs.widgets

Singleton {
    id: root
    readonly property var player: NacrePlayers.active
    readonly property bool shown: NacrePanelState.providerViews.some(view => (view.detached || NacrePanelState.hidden !== true) && (view.dashboard && view.dashboardTab === 2))
    readonly property bool spectrumVisible: NacrePanelState.providerViews.some(view => (view.detached || NacrePanelState.hidden !== true) && (view.mediaMini || view.dashboard && [0, 2].includes(view.dashboardTab)))
    readonly property string trackKey: JSON.stringify({
        title: player?.trackTitle || "",
        artist: player?.trackArtist || "",
        album: player?.trackAlbum || "",
        duration: player?.length || 0,
        art: player?.trackArtUrl || "",
        lyrics: NacreDashboardState.preferences.lyrics !== false,
        palette: NacreDashboardState.preferences.albumPalette !== false,
        mode: NacreColours.light ? "light" : "dark"
    })
    property string requestedKey: ""
    property string loadedKey: ""
    property var lines: []
    property var palette: ({})
    property string plain: ""
    property string message: ""
    property var spectrum: []
    readonly property string python: Quickshell.env("HOME") + "/.local/share/nacre/palette-runtime/venv/bin/python"
    readonly property string tool: (Quickshell.env("NACRE_DASHBOARD_TOOLS") || Quickshell.env("HOME") + "/.local/share/nacre/shell/tools") + "/dashboard-media.py"
    function load() {
        if (!shown || !player || details.running || loadedKey === trackKey)
            return;
        requestedKey = trackKey;
        details.command = [python, tool, requestedKey];
        details.running = true;
    }
    onTrackKeyChanged: {
        lines = [];
        palette = ({});
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
                const colors = {};
                for (const key of ["surface", "onSurface", "primary", "secondary", "tertiary"]) {
                    const value = data.palette?.[key];
                    if (typeof value === "string" && /^#?[0-9a-fA-F]{6}$/.test(value))
                        colors[key] = value.startsWith("#") ? value : "#" + value;
                }
                root.palette = Object.keys(colors).length === 5 ? colors : {};
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
