pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

Singleton {
    id: root
    property bool created: false
    property bool visible: false
    property var window: null
    readonly property var client: (NacreCompositor.windows || []).find(c => c.pid === Quickshell.processId && c.title === "Nacre Colors" && c.wmClass === "org.quickshell") || null
    readonly property bool active: visible && !!window && !window.minimized && (client?.workspace?.id ?? 1) > 0
    property var options: ({
            mode: "dark",
            personality: "natural",
            backgroundFromWallpaper: false,
            overrides: {},
            brightness: 0,
            saturation: 1,
            hour: 12,
            autoMode: false,
            tideAutomatic: false,
            timeTint: false,
            workspaceColors: false
        })
    property var preview: ({})
    property var library: []
    property var favorites: []
    property var history: []
    property var saved: ({
            wallpapers: {}
        })
    property int generation: 0
    property bool queued: false
    property bool helperReady: false
    property bool inFlight: false
    property int flightSequence: -1
    property string flightImage: ""
    property int previewRequests: 0
    property string helperFailure: ""
    property string error: ""
    property string status: ""
    property string exportDirectory: ""
    readonly property bool previewBusy: queued || inFlight
    readonly property bool actionBusy: actionJob.running
    readonly property string python: Quickshell.env("HOME") + "/.local/share/nacre/palette-runtime/venv/bin/python"
    readonly property string tool: Quickshell.env("HOME") + "/.local/share/nacre/shell/tools/colors.py"
    readonly property bool ready: !!preview.id && preview.request === options && !previewBusy && !error
    readonly property bool applied: !!preview.id && saved.lastApplied === preview.id && NacreWallpapers.actualCurrent === preview.image && NacrePresentation.active.colours?.surface === preview.palette?.colours?.surface
    function open(image = "") {
        const view = NacrePanelState.getForActive();
        if (view)
            NacrePanelState.closeTransient(view);
        created = true;
        visible = true;
        if (!catalogJob.running)
            catalogJob.running = true;
        if (image || !options.image)
            choose(image || NacreWallpapers.actualCurrent || NacreWallpapers.current);
        else if (!preview.id && !inFlight)
            changed();
        focusAttempts = 0;
        focusTimer.restart();
        if (window)
            window.activate();
    }
    function choose(image) {
        if (!image)
            return;
        const choice = Object.assign({}, saved.wallpapers?.[image] || {});
        if (choice.backgroundFromWallpaper === undefined && choice.personality)
            choice.backgroundFromWallpaper = choice.personality !== "natural";
        if (choice.personality === "source")
            choice.personality = "natural";
        options = Object.assign({
            mode: ["dark", "light"].includes(NacrePresentation.active.mode) ? NacrePresentation.active.mode : NacreColours.light ? "light" : "dark",
            personality: "natural",
            backgroundFromWallpaper: NacreWallpapers.appearancePreferences.paletteBackgroundFromWallpaper ?? false,
            overrides: {},
            brightness: 0,
            saturation: 1,
            hour: NacreTime.hours,
            autoMode: false,
            tideAutomatic: false,
            timeTint: false,
            workspaceColors: false
        }, choice, {
            image: image,
            compare: false
        });
        changed();
    }
    function close() {
        visible = false;
        focusTimer.stop();
        debounce.stop();
        queued = false;
        generation++;
        helperReady = false;
        inFlight = false;
        previewServer.running = false;
    }
    function change(values) {
        const next = Object.assign({}, options, values);
        if (Object.keys(values).some(key => ["personality", "backgroundFromWallpaper", "overrides", "brightness", "saturation", "accent", "pick"].includes(key)))
            delete next.favoriteId;
        if (JSON.stringify(next) === JSON.stringify(options) && !error && (preview.id || previewBusy))
            return;
        options = next;
        changed(Object.keys(values).some(key => ["brightness", "saturation", "overrides", "pick"].includes(key)) ? 60 : 0);
    }
    function setRole(role, value) {
        const overrides = Object.assign({}, options.overrides || {});
        if (value)
            overrides[role] = value;
        else
            delete overrides[role];
        change({
            overrides: overrides
        });
    }
    function useFavorite(id) {
        change({
            favoriteId: id
        });
    }
    function useHistory(item) {
        options = Object.assign({}, item.request);
        changed();
    }
    function changed(delay = 0) {
        generation++;
        if (inFlight && flightImage !== options.image) {
            helperReady = false;
            inFlight = false;
            previewServer.running = false;
        }
        queued = true;
        error = "";
        helperFailure = "";
        status = "";
        debounce.interval = delay;
        debounce.restart();
    }
    function startPreview() {
        if (inFlight || !queued || !visible || !options.image)
            return;
        if (!helperReady) {
            previewServer.running = true;
            return;
        }
        queued = false;
        inFlight = true;
        flightSequence = generation;
        flightImage = options.image;
        previewRequests++;
        previewServer.write(JSON.stringify({
            sequence: flightSequence,
            request: options
        }) + "\n");
    }
    function acceptPreview(text) {
        if (!visible)
            return;
        try {
            const response = JSON.parse(text);
            if (response.ready) {
                helperReady = true;
                startPreview();
                return;
            }
            if (response.sequence !== flightSequence || !inFlight)
                return;
            inFlight = false;
            if (response.sequence === generation) {
                const data = response.result;
                if (response.error)
                    error = response.error;
                else if (!data?.id || !data.palette?.colours)
                    error = "Invalid preview response";
                else {
                    preview = data;
                    options = data.request;
                }
            }
        } catch (failure) {
            inFlight = false;
            error = "Could not read the preview result";
        }
        if (queued)
            Qt.callLater(startPreview);
    }
    function action(name, extra = {}) {
        if (actionBusy || !ready)
            return;
        actionJob.action = name;
        actionJob.request = Object.assign({
            id: preview.id
        }, extra);
        actionJob.output = "";
        actionJob.failure = "";
        error = "";
        actionJob.running = true;
    }
    function minimize() {
        if (!client || minJob.running)
            return;
        minJob.command = NacreCompositor.hideWindowCommand(client.windowId);
        minJob.running = true;
    }
    property int focusAttempts: 0
    Timer {
        id: focusTimer
        interval: 80
        repeat: true
        onTriggered: {
            if (!root.visible || ++root.focusAttempts > 15) {
                stop();
                return;
            }
            if (minJob.running)
                return;
            const client = root.client;
            if (client && client.windowId) {
                if ((client.workspace?.id ?? 1) < 0)
                    NacreCompositor.moveWindowToWorkspace(client.windowId, Math.max(1, NacreCompositor.activeWsId));
                NacreCompositor.focusWindow(client.windowId);
                stop();
            }
        }
    }
    Timer {
        id: debounce
        interval: 90
        onTriggered: root.startPreview()
    }
    Process {
        id: minJob
        onExited: code => {
            if (code !== 0)
                root.error = "Could not minimize Colors.";
        }
    }
    Process {
        id: previewServer
        command: ["nice", "-n", "10", root.python, root.tool, "serve"]
        stdinEnabled: true
        stdout: SplitParser {
            onRead: text => root.acceptPreview(text)
        }
        stderr: SplitParser {
            onRead: line => root.helperFailure = line.slice(0, 240)
        }
        onExited: code => {
            root.helperReady = false;
            const failed = root.inFlight;
            root.inFlight = false;
            if (root.visible && root.queued)
                Qt.callLater(root.startPreview);
            else if (root.visible && failed)
                root.error = root.helperFailure || "Preview helper stopped; choose a style to retry.";
        }
    }
    Process {
        id: catalogJob
        command: [root.python, root.tool, "catalog"]
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    const d = JSON.parse(text);
                    root.library = d.entries;
                    if (!root.options.image)
                        root.choose(d.current || d.entries[0]?.path || "");
                    root.favorites = d.favorites;
                    root.history = d.history;
                } catch (error) {
                    root.error = "Could not read the wallpaper library.";
                }
            }
        }
    }
    Process {
        id: actionJob
        property string action: "apply"
        property var request: ({})
        property string output: ""
        property string failure: ""
        command: [root.python, root.tool, action]
        stdinEnabled: true
        environment: ({
                NACRE_PRESENTATION_STREAM: action === "apply" ? "1" : "0"
            })
        onStarted: write(JSON.stringify(request) + "\n")
        stdout: SplitParser {
            onRead: line => {
                try {
                    const data = JSON.parse(line);
                    if (data.presentation)
                        NacrePresentation.accept(data.presentation);
                    else
                        actionJob.output = line;
                } catch (failure) {
                    actionJob.failure = "Could not read the action result";
                }
            }
        }
        stderr: StdioCollector {
            onStreamFinished: actionJob.failure = text
        }
        onExited: code => {
            if (code !== 0) {
                root.error = (failure || "Action failed; try again.").slice(0, 240);
                return;
            }
            try {
                const data = JSON.parse(output);
                root.status = data.applied ? "Applied to the desktop" : data.saved ? "Saved " + data.saved : "Exported to " + data.directory;
                if (data.directory)
                    root.exportDirectory = data.directory;
                if (data.applied)
                    NacreWallpapers.refresh();
                if (!catalogJob.running)
                    catalogJob.running = true;
            } catch (error) {
                root.error = "Could not read the action result.";
            }
        }
    }
    FileView {
        path: Quickshell.env("HOME") + "/.config/nacre/colors.json"
        watchChanges: true
        printErrors: false
        onFileChanged: reload()
        onLoaded: {
            try {
                root.saved = JSON.parse(text());
            } catch (error) {
                root.error = "Saved colors could not be read.";
            }
        }
    }
    Connections {
        target: NacreTime
        function onHoursChanged() {
            const choice = root.saved.wallpapers?.[NacreWallpapers.actualCurrent];
            if (NacreWallpapers.preferences.palettePersonality !== "tide" || !choice?.tideAutomatic)
                return;
            const patch = {
                palettePersonality: "tide"
            };
            if (choice.autoMode)
                patch.paletteMode = NacreTime.hours >= 6 && NacreTime.hours < 19 ? "light" : "dark";
            NacreWallpapers.preference(patch);
        }
    }
    IpcHandler {
        target: "colorsApp"
        function open(image: string): void {
            root.open(image);
        }
        function preview(value: string): void {
            try {
                const values = JSON.parse(value);
                if (values.image && values.image !== root.options.image)
                    root.choose(values.image);
                root.change(values);
            } catch (error) {
                root.error = "Invalid preview request.";
            }
        }
        function close(): void {
            root.close();
        }
        function state(): string {
            return JSON.stringify({
                visible: root.visible,
                active: root.active,
                busy: root.previewBusy || root.actionBusy,
                previewRequests: root.previewRequests,
                options: root.options,
                previewHelperRunning: previewServer.running,
                image: root.options.image || "",
                personality: root.options.personality,
                backgroundFromWallpaper: root.options.backgroundFromWallpaper === true,
                previewId: root.preview.id || "",
                ready: root.ready,
                width: root.window?.width || 0,
                height: root.window?.height || 0,
                error: root.error,
                applied: root.applied
            });
        }
    }
}
