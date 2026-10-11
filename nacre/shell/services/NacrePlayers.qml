pragma Singleton
import Quickshell
import Quickshell.Io
import Quickshell.Hyprland
import Quickshell.Services.Mpris

Singleton {
    id: root
    readonly property var list: [...Mpris.players.values]
    property var manualActive: null
    readonly property var active: {
        if (manualActive && list.includes(manualActive))
            return manualActive;
        const playing = list.filter(player => player.isPlaying);
        return playing.find(player => player.identity === "Spotify") || playing[0] || list.find(player => player.identity === "Spotify") || list[0] || null;
    }
    onListChanged: if (manualActive && !list.includes(manualActive))
        manualActive = null
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
    GlobalShortcut {
        appid: "nacre_shell"
        name: "mediaToggle"
        onPressed: root.control("playPause")
    }
    GlobalShortcut {
        appid: "nacre_shell"
        name: "mediaNext"
        onPressed: root.control("next")
    }
    GlobalShortcut {
        appid: "nacre_shell"
        name: "mediaPrev"
        onPressed: root.control("previous")
    }
    GlobalShortcut {
        appid: "nacre_shell"
        name: "mediaStop"
        onPressed: root.control("stop")
    }
}
