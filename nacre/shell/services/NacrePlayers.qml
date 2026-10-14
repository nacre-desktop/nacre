pragma Singleton
import QtQuick
import QtQml.Models
import Quickshell
import qs.widgets
import Quickshell.Io
import "../modules/dashboard/media/media.js" as Model
import Quickshell.Services.Mpris

Singleton {
    id: root
    readonly property var list: [...Mpris.players.values]
    property var manualActive: null
    property var recentPlaying: null
    property var knownPlayers: []
    readonly property var localPlaying: list.filter(player => player.isPlaying && streamFor(player))
    onLocalPlayingChanged: if (manualActive && !localPlaying.includes(manualActive) && localPlaying.length)
        manualActive = null
    function streamFor(player) {
        return Model.playerStream(player, NacreAudio.playbackStreams);
    }
    function playbackChanged(player) {
        if (player.isPlaying) {
            recentPlaying = player;
            if (manualActive !== player)
                manualActive = null;
        } else if (manualActive === player && list.some(other => other !== player && other.isPlaying)) {
            manualActive = null;
        }
    }
    Instantiator {
        model: root.list
        delegate: Connections {
            required property var modelData
            target: modelData
            function onIsPlayingChanged() {
                root.playbackChanged(modelData);
            }
        }
    }
    readonly property var active: {
        if (manualActive && list.includes(manualActive))
            return manualActive;
        const playing = list.filter(player => player.isPlaying);
        const local = localPlaying;
        const preferred = local.length ? local : playing;
        return preferred.includes(recentPlaying) ? recentPlaying : preferred[0] || list[0] || null;
    }
    onListChanged: {
        for (const player of list.filter(actor => !knownPlayers.includes(actor)))
            if (player.isPlaying)
                playbackChanged(player);
        knownPlayers = list;
        if (manualActive && !list.includes(manualActive))
            manualActive = null;
        if (recentPlaying && !list.includes(recentPlaying))
            recentPlaying = null;
    }
    function control(action) {
        const player = active;
        if (!player || !list.includes(player))
            return false;
        const allowed = {
            play: "canPlay",
            pause: "canPause",
            playPause: "canTogglePlaying",
            previous: "canGoPrevious",
            next: "canGoNext",
            stop: "canControl"
        };
        if (!allowed[action] || !player[allowed[action]])
            return false;
        const method = action === "playPause" ? "togglePlaying" : action;
        player[method]();
        return true;
    }
    IpcHandler {
        target: "mpris"
        function getActive(prop: string): string {
            const value = root.active?.[prop];
            return ["string", "number", "boolean"].includes(typeof value) ? String(value) : "";
        }
        function list(): string {
            return root.list.map(player => player.identity).join("\n");
        }
        function play(): void {
            root.control("play");
        }
        function pause(): void {
            root.control("pause");
        }
        function playPause(): void {
            root.control("playPause");
        }
        function previous(): void {
            root.control("previous");
        }
        function next(): void {
            root.control("next");
        }
        function stop(): void {
            root.control("stop");
        }
    }
    NacreShortcut {
        appid: "nacre_shell"
        name: "mediaToggle"
        onPressed: root.control("playPause")
    }
    NacreShortcut {
        appid: "nacre_shell"
        name: "mediaNext"
        onPressed: root.control("next")
    }
    NacreShortcut {
        appid: "nacre_shell"
        name: "mediaPrev"
        onPressed: root.control("previous")
    }
    NacreShortcut {
        appid: "nacre_shell"
        name: "mediaStop"
        onPressed: root.control("stop")
    }
}
