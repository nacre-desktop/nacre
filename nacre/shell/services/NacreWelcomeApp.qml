pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

Singleton {
    id: root
    property bool created: false
    property bool visible: false
    property var window: null
    property string page: "welcome"
    readonly property var steps: ["welcome", "colors", "shortcuts", "apps", "ready"]
    readonly property int step: Math.max(0, steps.indexOf(page))
    property var data: ({
            preferences: {
                showAtLogin: true
            },
            available: {}
        })
    property string error: ""
    property string optionalApp: ""
    property var demoEntries: []
    property string demoSnapshot: ""
    property int demoGeneration: 0
    property bool demoChanged: false
    property bool demoRestored: false
    property bool demoStarting: false
    property string requestedMode: ""
    property string requestedPersonality: ""
    readonly property bool demoBusy: demoStarting || NacreWallpapers.demoRestoring || NacreWallpapers.themeBusy || !!NacreWallpapers.selectedPath || !!NacreWallpapers.queuedPath
    readonly property var appearance: NacreWallpapers.appearancePreferences
    readonly property string demoError: error || NacreWallpapers.error || ""
    property var queue: []
    property int focusAttempts: 0
    readonly property bool busy: worker.running
    readonly property var client: (NacreCompositor.windows || []).find(c => c.pid === Quickshell.processId && c.title === "Nacre Welcome" && c.wmClass === "org.quickshell") || null
    readonly property bool active: visible && !!window && !window.minimized && (client?.workspace?.id ?? 1) > 0
    function prepareDemo() {
        if (demoStarting || demoSnapshot || NacreWallpapers.themeBusy || NacreWallpapers.selectedPath || NacreWallpapers.queuedPath)
            return;
        demoStarting = true;
        request(["demo-start", String(demoGeneration)]);
    }
    function restoreDemo() {
        if (demoBusy || !demoSnapshot || !demoChanged)
            return;
        NacreWallpapers.restoreDemo(demoSnapshot);
    }
    function selectDemo(path) {
        if (demoBusy || !demoSnapshot || !demoEntries.some(item => item.path === path))
            return;
        if (appearance.palettePreset !== "wallpaper")
            NacreWallpapers.preference({
                palettePreset: "wallpaper"
            });
        demoChanged = true;
        demoRestored = false;
        NacreWallpapers.setWallpaper(path);
    }
    function themeDemo(mode, personality) {
        if (demoBusy || !demoSnapshot || !NacreWallpapers.actualCurrent)
            return;
        const value = {
            palettePreset: "wallpaper"
        };
        if (["dark", "light"].includes(mode)) {
            requestedMode = mode;
            value.paletteMode = mode;
        }
        if (["natural", "pigment", "harmony", "pop", "mist", "vivid", "pearl", "tide"].includes(personality)) {
            requestedPersonality = personality;
            value.palettePersonality = personality;
        }
        if (Object.keys(value).length > 1) {
            demoChanged = true;
            demoRestored = false;
            NacreWallpapers.preference(value);
        }
    }
    function setup(name) {
        if (["ai", "brain"].includes(name)) {
            optionalApp = name;
            page = "apps";
        }
    }
    Connections {
        target: NacreWallpapers
        function onThemeBusyChanged() {
            if (!NacreWallpapers.themeBusy) {
                root.requestedMode = "";
                root.requestedPersonality = "";
            }
            root.captureWhenReady();
        }
        function onSelectedPathChanged() {
            root.captureWhenReady();
        }
        function onDemoRestored() {
            root.demoChanged = false;
            root.demoRestored = true;
        }
    }
    function captureWhenReady() {
        if (visible && !demoSnapshot && !demoStarting)
            Qt.callLater(root.prepareDemo);
    }
    Connections {
        target: NacreCompositor
        function onConfigurationReloaded() {
            if (root.active && root.page === "shortcuts")
                root.refresh();
        }
    }
    onPageChanged: if (visible && page === "shortcuts")
        refresh()
    function request(args) {
        queue = [...queue, args];
        next();
    }
    function next() {
        if (!worker.running && queue.length) {
            const args = queue[0];
            queue = queue.slice(1);
            worker.job = args;
            worker.command = args[0] === "demo-start" ? ["python3", NacreWallpapers.tool, "demo-start"] : ["python3", Quickshell.env("HOME") + "/.local/share/nacre/shell/tools/welcome.py", ...args];
            worker.running = true;
        }
    }
    function refresh() {
        if (!created)
            return;
        if (!queue.some(args => args[0] === "state"))
            request(["state"]);
    }
    function setStartup(value) {
        error = "";
        request(["set-startup", JSON.stringify(value)]);
    }
    function jump(name) {
        page = steps.includes(name) || name === "help" ? name : "welcome";
        optionalApp = "";
    }
    function advance() {
        if (page === "help")
            jump("ready");
        else if (step < steps.length - 1)
            jump(steps[step + 1]);
        else
            finish();
    }
    function back() {
        if (page === "help")
            jump("ready");
        else if (step > 0)
            jump(steps[step - 1]);
        else
            close();
    }
    function finish() {
        request(["finish-tour"]);
        close();
    }
    function open(section = "home") {
        jump(section === "home" ? "welcome" : section);
        NacrePanelState.clearPopouts();
        const view = NacrePanelState.getForActive();
        if (view)
            NacrePanelState.closeTransient(view);
        const newSession = !visible;
        created = true;
        visible = true;
        optionalApp = "";
        if (newSession) {
            requestedMode = "";
            requestedPersonality = "";
            demoGeneration++;
            demoEntries = [];
            demoSnapshot = "";
            demoChanged = false;
            demoRestored = false;
            demoStarting = false;
            error = "";
        }
        prepareDemo();
        refresh();
        focusAttempts = 0;
        if (window)
            window.activate();
        focusTimer.restart();
    }
    function close() {
        visible = false;
        focusTimer.stop();
    }
    function route(name) {
        if (name === "colors") {
            close();
            NacreColorsApp.open("");
        } else if (name === "wallpaper" || name === "launcher") {
            close();
            NacrePanelState.openMode(name === "wallpaper" ? "wallpaper" : "apps", "", false);
        } else if (name.startsWith("settings:")) {
            const page = name.slice(9);
            if (["appearance", "desktop", "displays", "time", "network", "sound", "ai", "maintenance"].includes(page)) {
                close();
                NacreSettingsApp.open(page);
            }
        } else if (name === "ai" && data.available?.ai) {
            close();
            Quickshell.execDetached(["uwsm", "app", "--", "nacre-ai"]);
        } else if (name === "brain" && data.available?.brain) {
            close();
            Quickshell.execDetached(["uwsm", "app", "--", "nacre-brain", "open"]);
        }
    }
    function openCredit(url) {
        if (typeof url === "string" && url.startsWith("https://") && demoEntries.some(item => item.source === url || item.licenseUrl === url))
            Quickshell.execDetached(["uwsm", "app", "--", "xdg-open", url]);
    }
    function link(name) {
        const urls = {
            project: "https://github.com/nacre-desktop/nacre",
            hyprland: "https://wiki.hypr.land/",
            quickshell: "https://quickshell.org/docs/",
            cachyos: "https://wiki.cachyos.org/"
        };
        if (urls[name])
            Quickshell.execDetached(["uwsm", "app", "--", "xdg-open", urls[name]]);
    }
    Timer {
        id: focusTimer
        interval: 80
        repeat: true
        onTriggered: {
            const client = root.client;
            if (!root.visible || ++root.focusAttempts > 12) {
                stop();
                return;
            }
            if (client && client.windowId) {
                if ((client.workspace?.id ?? 1) < 0)
                    NacreCompositor.moveWindowToWorkspace(client.windowId, Math.max(1, NacreCompositor.activeWsId));
                NacreCompositor.focusWindow(client.windowId);
                stop();
            }
        }
    }
    Process {
        id: worker
        property var job: []
        stdout: SplitParser {
            splitMarker: ""
            onRead: line => {
                try {
                    const value = JSON.parse(line);
                    if (value.error)
                        root.error = value.error;
                    else if (worker.job[0] === "demo-start") {
                        if (Number(worker.job[1]) === root.demoGeneration) {
                            root.demoEntries = value.demoEntries;
                            root.demoSnapshot = value.snapshot;
                            root.error = "";
                        }
                    } else {
                        root.data = value;
                        root.error = "";
                    }
                } catch (e) {
                    root.error = "Could not read Welcome preferences. Reopen Welcome to try again.";
                }
            }
        }
        onExited: exitCode => {
            if (worker.job[0] === "demo-start" && Number(worker.job[1]) === root.demoGeneration)
                root.demoStarting = false;
            if (exitCode !== 0 && !root.error)
                root.error = "Welcome could not save its preference. Try reopening the app.";
            root.next();
        }
    }
    IpcHandler {
        target: "welcomeApp"
        function open(page: string): void {
            root.open(page);
        }
        function close(): void {
            root.close();
        }
        function refresh(): void {
            root.refresh();
        }
        function setStartup(value: bool): void {
            root.setStartup(value);
        }
        function route(name: string): void {
            root.route(name);
        }
        function selectDemo(path: string): void {
            root.selectDemo(path);
        }
        function restoreDemo(): void {
            root.restoreDemo();
        }
        function themeDemo(mode: string, personality: string): void {
            root.themeDemo(mode, personality);
        }
        function state(): string {
            return JSON.stringify({
                created: root.created,
                visible: root.visible,
                active: root.active,
                page: root.page,
                busy: root.busy,
                error: root.error,
                data: root.data,
                demoEntries: root.demoEntries.map(item => ({
                            path: item.path,
                            name: item.name,
                            artist: item.artist,
                            license: item.license
                        })),
                demoBusy: root.demoBusy,
                demoSnapshotReady: !!root.demoSnapshot,
                demoChanged: root.demoChanged,
                demoRestored: root.demoRestored,
                appearance: root.appearance,
                optionalApp: root.optionalApp,
                width: root.window?.width || 0,
                height: root.window?.height || 0
            });
        }
    }
}
