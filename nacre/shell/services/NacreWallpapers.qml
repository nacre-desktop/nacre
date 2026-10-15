pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io
import qs.utils
import "../utils/scripts/wallpaper-rotation.js" as Rotation
import "app-search.js" as Search

Singleton {
    id: root
    readonly property string currentNamePath: NacrePaths.state.slice(7) + "/wallpaper/last.txt"
    readonly property string path: NacrePaths.pictures.slice(7) + "/Wallpapers"
    readonly property string tool: "wallpaper-media"
    property var list: []
    readonly property bool demoRestoring: demoRestore.running
    signal demoRestored
    property var preferences: ({
            kind: "static",
            layout: "carousel",
            paused: false,
            motionMode: "full",
            rotationEnabled: false,
            rotationMinutes: 30,
            rotationKind: "all",
            rotationShuffle: true,
            palettePreset: "wallpaper",
            paletteMode: "dark",
            paletteAutoMode: false,
            paletteHarmony: false
        })
    property var palettePresets: []
    readonly property var paletteOptions: NacrePresentation.active.paletteOptions ?? []
    readonly property var appearancePreferences: Object.assign({}, preferences, NacrePresentation.active.colours ? {
        palettePreset: NacrePresentation.active.palettePreset ?? preferences.palettePreset,
        paletteMode: NacrePresentation.active.mode ?? preferences.paletteMode,
        paletteHarmony: NacrePresentation.active.paletteHarmony ?? preferences.paletteHarmony,
        palettePersonality: NacrePresentation.active.palettePersonality ?? preferences.palettePersonality,
        paletteBackgroundFromWallpaper: NacrePresentation.active.paletteBackgroundFromWallpaper ?? preferences.paletteBackgroundFromWallpaper
    } : {})
    readonly property string selectedAccent: NacrePresentation.active.selectedAccent ?? ""
    property var media: ({})
    property string lastImage: ""
    property string selectedPath: ""
    property string queuedPath: ""
    property string actionError: ""
    property string catalogError: ""
    readonly property string error: actionError || catalogError
    property string watchError: ""
    property bool refreshQueued: false
    property bool catalogReceived: false
    property bool commitReceived: false
    property bool preferenceReceived: false
    property bool importReceived: false
    property bool warmQueued: false
    property var preferenceQueue: []
    property int watchRetrySeconds: 2
    readonly property bool loading: catalog.running
    readonly property bool themeBusy: demoRestore.running || prefWorker.running || preferenceQueue.length > 0
    readonly property string actualCurrent: media.path && media.poster === lastImage ? media.path : lastImage
    readonly property string current: selectedPath || actualCurrent
    readonly property var currentEntry: (media.path === current && media.preview ? media : list.find(entry => entry.path === current)) || (media.path === current ? media : null)
    readonly property string poster: currentEntry?.poster || current
    readonly property string preview: currentEntry?.preview || poster
    readonly property string thumbnail: currentEntry?.thumbnail || poster
    readonly property string pendingPoster: NacrePresentation.pending.poster || poster
    readonly property var displayEntry: (media.poster === NacrePresentation.active.poster ? media : (list.concat(NacreWelcomeApp.demoEntries || [])).find(entry => entry.poster === NacrePresentation.active.poster)) || null
    readonly property string displayPath: displayEntry?.path || actualCurrent
    readonly property string displayPreview: displayEntry?.preview || NacrePresentation.active.poster || preview
    readonly property bool displayDynamic: displayEntry?.dynamic === true
    readonly property bool displayAnimated: displayEntry?.animated === true
    readonly property bool dynamic: currentEntry?.dynamic === true
    readonly property bool animated: currentEntry?.animated === true

    property var wallpaperHistory: []
    property string historyTraversalPath: ""
    property var rotationRemaining: []
    readonly property var rotationPool: Rotation.pool(list, preferences.rotationKind || "all")
    readonly property string rotationKey: (preferences.rotationKind || "all") + ":" + (preferences.rotationShuffle ?? true) + ":" + rotationPool.join("|")
    readonly property bool pickerOpen: NacrePanelState.providerViews.some(view => view.launcher && view.launcherMode === "wallpaper")
    readonly property bool appearanceOpen: NacrePanelState.settingsVisible && ["wallpaper", "colors", "appearance"].includes(NacrePanelState.settingsPage.split("#")[0].split("/")[0])
    readonly property bool rotationReady: rotationPool.length > 1 && !WallpaperPlayback.sleeping && !WallpaperPlayback.locked && !commit.running && !themeBusy && !catalog.running && !selectedPath && !queuedPath
    readonly property bool welcomeDemoOpen: NacreWelcomeApp.active
    readonly property bool canRotate: rotationReady && !pickerOpen && !appearanceOpen && !welcomeDemoOpen
    readonly property bool rotationAllowed: preferences.rotationEnabled === true && canRotate
    readonly property real rotationIntervalMs: Math.max(5, Math.min(1440, Number(preferences.rotationMinutes) || 30)) * 60000
    property real rotationStartupMs: Date.now()
    property real rotationRetryMs: 0
    readonly property real rotationAnchorMs: Math.max(media.appliedAtMs || 0, NacrePresentation.active.changedAtMs || 0, preferences.rotationAnchorMs || 0) || rotationStartupMs
    readonly property real rotationDueMs: Math.max(rotationAnchorMs + rotationIntervalMs, rotationRetryMs)
    readonly property string rotationPauseReason: WallpaperPlayback.sleeping ? "asleep" : WallpaperPlayback.locked ? "locked" : pickerOpen ? "wallpaper picker open" : appearanceOpen ? "Settings open" : welcomeDemoOpen ? "Welcome demo open" : rotationPool.length < 2 ? "fewer than two wallpapers" : !rotationReady ? "applying changes" : ""
    readonly property string rotationStatus: !preferences.rotationEnabled ? NacreI18n.tr("wallpaper.rotation.off", "Rotation is off") : rotationPauseReason ? NacreI18n.option("rotation-pause", rotationPauseReason) : NacreI18n.tr("wallpaper.rotation.next", "Next wallpaper at {time}", {
        time: NacreI18n.time(new Date(rotationDueMs), "hh:mm")
    })

    function localPath(value) {
        return typeof value === "string" && value.startsWith("/") && !value.includes("\0") && value.length <= 16384;
    }
    function validEntry(entry) {
        return entry && localPath(entry.path) && localPath(entry.poster) && typeof entry.name === "string" && typeof entry.dynamic === "boolean" && typeof entry.animated === "boolean" && (!entry.preview || localPath(entry.preview)) && (!entry.thumbnail || localPath(entry.thumbnail));
    }
    function result(text) {
        if (text.length > 16777216)
            throw new Error("Wallpaper response too large");
        const data = JSON.parse(text);
        if (!data || typeof data !== "object" || Array.isArray(data))
            throw new Error("Invalid wallpaper response");
        if (data.error)
            throw new Error(String(data.error).slice(0, 240));
        return data;
    }
    function publishCatalog(text) {
        try {
            const data = result(text);
            if (!Array.isArray(data.entries) || data.entries.length > 10000 || !data.entries.every(validEntry) || !data.preferences || typeof data.preferences !== "object" || Array.isArray(data.preferences) || !Array.isArray(data.palettes))
                throw new Error("Invalid wallpaper catalogue");
            const seen = new Set();
            list = data.entries.filter(entry => {
                if (seen.has(entry.path))
                    return false;
                seen.add(entry.path);
                return true;
            });
            if (!prefWorker.running && !preferenceQueue.length)
                preferences = data.preferences;
            palettePresets = data.palettes;
            if (validEntry(data.media))
                media = data.media;
            catalogError = data.errors?.length ? data.errors.length + " wallpaper files could not be read." : "";
            return true;
        } catch (failure) {
            catalogError = String(failure.message).slice(0, 240);
            return false;
        }
    }
    function fuzzyQuery(query) {
        return Search.query(list.map(entry => Search.prepare(Object.assign({}, entry, {
                id: entry.path
            }))), query);
    }
    function refresh() {
        if (catalog.running) {
            refreshQueued = true;
            return;
        }
        catalogReceived = false;
        catalog.running = true;
    }
    function prepareCache() {
        if (warmer.running) {
            warmQueued = true;
            return;
        }
        warmQueued = false;
        warmer.running = true;
    }
    function browse(value) {
        if (!localPath(value))
            return;
        setWallpaper(value);
    }
    function commitSelection() {
        if (selectedPath && (!commit.running || commit.requestPath !== selectedPath)) {
            queuedPath = selectedPath;
            selectionDelay.stop();
            startCommit();
        }
    }
    function restoreDemo(token) {
        if (themeBusy || commit.running || selectedPath || queuedPath || !/^[0-9a-f]{32}$/.test(token))
            return;
        demoRestore.token = token;
        demoRestore.received = false;
        actionError = "";
        demoRestore.running = true;
    }
    function setWallpaper(value) {
        if (!localPath(value)) {
            actionError = "Choose a local wallpaper file.";
            return;
        }
        selectedPath = value;
        queuedPath = value;
        selectionDelay.restart();
    }
    function startCommit() {
        if (demoRestore.running || commit.running || prefWorker.running || !queuedPath)
            return;
        if (historyTraversalPath !== queuedPath)
            historyTraversalPath = "";
        commit.previousPath = actualCurrent;
        commit.requestPath = queuedPath;
        queuedPath = "";
        commitReceived = false;
        actionError = "";
        commit.running = true;
    }
    function receivePresentation(text) {
        try {
            const data = JSON.parse(text);
            if (!data.presentation)
                return false;
            NacrePresentation.accept(data.presentation);
            return true;
        } catch (failure) {
            return false;
        }
    }
    function publishSelection(text) {
        try {
            const data = result(text);
            if (!validEntry(data) || data.path !== commit.requestPath)
                throw new Error("Invalid wallpaper selection response");
            const previous = commit.previousPath;
            if (previous && previous !== data.path) {
                if (historyTraversalPath === data.path)
                    wallpaperHistory = wallpaperHistory.slice(0, wallpaperHistory.lastIndexOf(data.path));
                else
                    wallpaperHistory = [...wallpaperHistory, previous].slice(-100);
            }
            historyTraversalPath = "";
            media = data;
            lastImage = data.poster;
            if (selectedPath === commit.requestPath)
                selectedPath = "";
            rotationRetryMs = 0;
            return true;
        } catch (failure) {
            actionError = String(failure.message).slice(0, 240);
            return false;
        }
    }
    function preference(value) {
        if (!value || typeof value !== "object" || Array.isArray(value) || !Object.keys(value).length)
            return;
        const next = JSON.parse(JSON.stringify(value));
        if (next.paletteHarmony !== undefined && next.palettePersonality === undefined)
            next.palettePersonality = next.paletteHarmony ? "harmony" : "natural";
        if (next.palettePersonality !== undefined)
            delete next.paletteHarmony;
        const queue = preferenceQueue.slice();
        const palette = Object.keys(next).some(key => key.startsWith("palette"));
        const tail = queue[queue.length - 1];
        if (tail && Object.keys(tail).some(key => key.startsWith("palette")) === palette)
            queue[queue.length - 1] = Object.assign({}, tail, next);
        else
            queue.push(next);
        preferenceQueue = queue;
        nextPreference();
    }
    function nextPreference() {
        if (demoRestore.running || prefWorker.running || commit.running || !preferenceQueue.length)
            return;
        prefWorker.value = preferenceQueue[0];
        preferenceQueue = preferenceQueue.slice(1);
        preferenceReceived = false;
        prefWorker.running = true;
    }
    function publishPreference(text) {
        try {
            const data = result(text);
            if (typeof data.kind !== "string" || typeof data.layout !== "string")
                throw new Error("Invalid wallpaper preferences");
            preferences = data;
            actionError = "";
            return true;
        } catch (failure) {
            actionError = String(failure.message).slice(0, 240);
            return false;
        }
    }
    function addFiles(paths) {
        if (importer.running || !Array.isArray(paths) || !paths.length)
            return;
        importer.files = paths.map(String);
        importer.action = "import";
        importReceived = false;
        importer.running = true;
    }
    function pickFiles() {
        if (importer.running)
            return;
        importer.files = [];
        importer.action = "pick";
        importReceived = false;
        importer.running = true;
    }
    function scheduleRotation() {
        rotationTimer.stop();
        if (!rotationAllowed)
            return;
        rotationTimer.interval = Math.max(1, Math.min(2147483647, rotationDueMs - Date.now()));
        rotationTimer.start();
    }
    function advanceRotation(manual) {
        if (manual ? !rotationReady : !rotationAllowed)
            return;
        const choice = Rotation.next(rotationPool, actualCurrent, preferences.rotationShuffle !== false, rotationRemaining, Math.random());
        rotationRemaining = choice.remaining;
        if (choice.path) {
            queuedPath = choice.path;
            startCommit();
        }
    }
    function previousWallpaper() {
        if (!rotationReady)
            return;
        const valid = wallpaperHistory.filter(path => rotationPool.includes(path));
        while (valid.length && valid[valid.length - 1] === actualCurrent)
            valid.pop();
        wallpaperHistory = valid;
        const index = rotationPool.indexOf(actualCurrent);
        const choice = valid.length ? valid[valid.length - 1] : rotationPool[(index - 1 + rotationPool.length) % rotationPool.length];
        historyTraversalPath = valid.length ? choice : "";
        queuedPath = choice;
        startCommit();
    }
    function toggleMotion() {
        if (displayDynamic && !themeBusy)
            preference({
                paused: !WallpaperPlayback.paused
            });
    }
    onRotationAllowedChanged: scheduleRotation()
    onRotationDueMsChanged: scheduleRotation()
    onRotationKeyChanged: rotationRemaining = []
    Component.onCompleted: refresh()

    Timer {
        id: selectionDelay
        interval: 35
        onTriggered: root.startCommit()
    }
    Timer {
        id: rotationTimer
        objectName: "wallpaperRotationTimer"
        onTriggered: root.advanceRotation(false)
    }
    Timer {
        id: followup
        interval: 1
        onTriggered: {
            root.nextPreference();
            root.startCommit();
        }
    }
    FileView {
        path: root.currentNamePath
        printErrors: false
        watchChanges: true
        onFileChanged: reload()
        onLoaded: {
            const value = text().trim();
            if (root.localPath(value))
                root.lastImage = value;
        }
    }
    FileView {
        path: NacrePaths.state + "/wallpaper/media.json"
        printErrors: false
        watchChanges: true
        onFileChanged: reload()
        onLoaded: {
            try {
                const data = root.result(text());
                if (root.validEntry(data))
                    root.media = data;
            } catch (failure) {
                console.warn("NacreWallpapers: Cannot decode current wallpaper state:", failure);
            }
        }
    }
    Process {
        id: catalog
        objectName: "wallpaperCatalog"
        command: [NacrePaths.helperBinary, root.tool, "catalog"]
        stdout: StdioCollector {
            onStreamFinished: root.catalogReceived = root.publishCatalog(text)
        }
        onExited: (code, status) => {
            if ((code !== 0 || !root.catalogReceived) && !root.error)
                root.catalogError = "Could not refresh wallpapers; the last catalogue is still shown.";
            if (root.refreshQueued) {
                root.refreshQueued = false;
                catalogDelay.start();
            } else if (root.catalogReceived)
                root.prepareCache();
        }
    }
    Timer {
        id: catalogDelay
        interval: 100
        onTriggered: root.refresh()
    }
    Process {
        id: demoRestore
        property string token: ""
        property bool received: false
        command: [NacrePaths.helperBinary, root.tool, "demo-restore", token]
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    const data = root.result(text);
                    if (!root.validEntry(data.media) || typeof data.preferences?.kind !== "string")
                        throw new Error("Invalid demo restore response");
                    root.media = data.media;
                    root.lastImage = data.media.poster;
                    root.preferences = data.preferences;
                    demoRestore.received = true;
                } catch (failure) {
                    root.actionError = String(failure.message).slice(0, 240);
                }
            }
        }
        onExited: code => {
            if (code === 0 && received)
                root.demoRestored();
            else if (!root.error)
                root.actionError = "Could not restore the starting desktop. Try again.";
            followup.start();
        }
    }
    NacrePaletteTask {
        id: commit
        objectName: "wallpaperCommit"
        property string requestPath: ""
        property string previousPath: ""
        command: [NacrePaths.helperBinary, root.tool, "select", requestPath, "--stream-presentation"]
        readonly property Connections output: Connections {
            target: commit.stdout
            function onRead(data) {
                if (!root.receivePresentation(data))
                    root.commitReceived = root.publishSelection(data);
            }
        }
        onExited: (code, status) => {
            if (code !== 0 || !root.commitReceived) {
                if (!root.error)
                    root.actionError = "Wallpaper change failed. Try again.";
                if (root.selectedPath === requestPath)
                    root.selectedPath = "";
                root.rotationRetryMs = Date.now() + 60000;
                root.historyTraversalPath = "";
            }
            followup.start();
        }
    }
    NacrePaletteTask {
        id: prefWorker
        transport: commit
        objectName: "wallpaperPreferences"
        property var value: ({})
        command: [NacrePaths.helperBinary, root.tool, Object.keys(value).some(key => key.startsWith("palette")) ? "theme" : "preferences", "--stream-presentation"]
        stdinEnabled: true
        onStarted: write(JSON.stringify(value) + "\n")
        readonly property Connections output: Connections {
            target: prefWorker.stdout
            function onRead(data) {
                if (!root.receivePresentation(data))
                    root.preferenceReceived = root.publishPreference(data);
            }
        }
        onExited: (code, status) => {
            if ((code !== 0 || !root.preferenceReceived) && !root.error)
                root.actionError = "Could not save wallpaper preferences. Try again.";
            root.refresh();
            followup.start();
        }
    }
    Process {
        id: importer
        objectName: "wallpaperImporter"
        property var files: []
        property string action: "pick"
        command: [NacrePaths.helperBinary, root.tool, action]
        stdinEnabled: action === "import"
        onStarted: if (action === "import")
            write(JSON.stringify(files) + "\n")
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    const data = root.result(text);
                    root.importReceived = data.cancelled === true || Array.isArray(data.imported);
                    if (!root.importReceived)
                        throw new Error("Invalid wallpaper import response");
                    if (!data.cancelled)
                        root.refresh();
                } catch (failure) {
                    root.actionError = String(failure.message).slice(0, 240);
                }
            }
        }
        onExited: (code, status) => {
            if ((code !== 0 || !root.importReceived) && !root.error)
                root.actionError = "Could not import wallpapers. Try again.";
        }
    }
    Process {
        id: warmer
        objectName: "wallpaperWarmer"
        command: ["nice", "-n", "19", NacrePaths.helperBinary, root.tool, "warm"]
        stdout: StdioCollector {}
        onExited: {
            if (root.warmQueued)
                warmDelay.start();
        }
    }
    Timer {
        id: warmDelay
        interval: 2500
        onTriggered: root.prepareCache()
    }
    Process {
        id: watcher
        objectName: "wallpaperWatcher"
        command: [NacrePaths.helperBinary, "wallpaper-watch", "--engine", NacrePaths.helperBinary]
        running: true
        stdout: SplitParser {
            onRead: {
                root.watchError = "";
                catalogDelay.restart();
            }
        }
        onStarted: watchStable.start()
        onExited: {
            watchStable.stop();
            root.watchError = "Wallpaper directory monitoring paused; retrying.";
            watchRetry.interval = root.watchRetrySeconds * 1000;
            root.watchRetrySeconds = Math.min(60, root.watchRetrySeconds * 2);
            watchRetry.start();
        }
    }
    Timer {
        id: watchStable
        interval: 30000
        onTriggered: root.watchRetrySeconds = 2
    }
    Timer {
        id: watchRetry
        onTriggered: watcher.running = true
    }
    IpcHandler {
        target: "wallpaper"
        function get(): string {
            return root.current;
        }
        function set(path: string): void {
            root.setWallpaper(path);
        }
        function configure(value: string): void {
            try {
                root.preference(JSON.parse(value));
            } catch (failure) {
                root.actionError = "Invalid wallpaper preference JSON.";
            }
        }
        function next(): void {
            root.advanceRotation(true);
        }
        function previous(): void {
            root.previousWallpaper();
        }
        function toggleMotion(): void {
            root.toggleMotion();
        }
        function list(): string {
            return root.list.map(entry => entry.path).join("\n");
        }
        function state(): string {
            return JSON.stringify({
                selected: root.current,
                applied: root.actualCurrent,
                queued: root.queuedPath,
                busy: commit.running || root.themeBusy,
                rotationAllowed: root.rotationAllowed,
                rotationPauseReason: root.rotationPauseReason,
                rotationPool: root.rotationPool.length,
                rotationRemainingSeconds: Math.max(0, Math.ceil((root.rotationDueMs - Date.now()) / 1000)),
                rotationTimerRunning: rotationTimer.running,
                current: root.current,
                poster: root.poster,
                preview: root.preview,
                thumbnail: root.thumbnail,
                dynamic: root.dynamic,
                animated: root.animated,
                preferences: root.preferences,
                loading: root.loading,
                error: root.error,
                count: root.list.length,
                rotationStatus: root.rotationStatus,
                rotationDueMs: root.rotationDueMs,
                watcherRunning: watcher.running,
                watchError: root.watchError
            });
        }
    }
}
