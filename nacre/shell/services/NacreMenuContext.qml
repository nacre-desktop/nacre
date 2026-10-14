import QtQuick
import qs.utils
import Quickshell
import Quickshell.Io
import Quickshell.Services.UPower
import "../modules/launcher/menu-model.js" as Model
import "../modules/launcher/launcher-sources.js" as Sources
import "../modules/launcher/app-browser.js" as Legacy

Scope {
    id: root
    property var shared: null
    readonly property NacreLauncherWorkflows workflowOwner: NacreLauncherWorkflows {
        active: !root.shared
    }
    readonly property var workflows: shared ? shared.workflows : workflowOwner
    readonly property NacreLauncherPreview previews: NacreLauncherPreview {
        helper: root.workflows.helper
    }
    readonly property NacreSoftwareLibrary libraryOwner: NacreSoftwareLibrary {
        active: !root.shared
    }
    readonly property var library: shared ? shared.library : libraryOwner
    readonly property Connections libraryChanges: Connections {
        target: root.library
        function onCompleted(operation, success) {
            if (success && (operation.startsWith("theme-") || operation === "extension-theme"))
                NacreWallpapers.refresh();
        }
    }
    property var localSourceOptions: Sources.defaults()
    readonly property var sourceOptions: shared ? shared.sourceOptions : localSourceOptions
    onSourceOptionsChanged: {
        if (!sourceOptions.files) {
            fileWanted = "";
            fileDelay.stop();
            fileWorker.running = false;
            fileRows = [];
            fileQuery = "";
        }
        if (!sourceOptions.brain) {
            brainWanted = "";
            brainDelay.stop();
            brainWorker.running = false;
            notes = [];
            brainQuery = "";
        }
        if (!sourceOptions.calculator) {
            conversionWanted = "";
            conversionDelay.stop();
            conversionWorker.running = false;
            conversion = ({
                    ok: false
                });
        }
    }
    FileView {
        path: root.shared ? "" : NacrePaths.config + "/launcher-sources.json"
        preload: !root.shared
        watchChanges: true
        printErrors: false
        onFileChanged: reload()
        onLoaded: root.localSourceOptions = Sources.decode(text())
        onLoadFailed: root.localSourceOptions = Sources.defaults()
    }
    property var localEmojiCatalog: []
    readonly property var emojiCatalog: shared ? shared.emojiCatalog : localEmojiCatalog
    property string brainWanted: ""
    property string brainQuery: ""
    property var notes: []
    readonly property string tools: NacrePaths.tools
    property string defaultText: ""
    property string extensionText: ""
    property var catalog: shared ? shared.catalog : ({
            rows: [],
            software: [],
            error: ""
        })
    property var snapshot: shared ? shared.snapshot : ({
            capabilities: {},
            packages: [],
            fonts: [],
            reminders: [],
            webapps: []
        })
    property string fileWanted: ""
    property string fileQuery: ""
    property var fileRows: []
    property string fileMessage: ""
    property string conversionWanted: ""
    property string conversionQuery: ""
    property var conversion: ({
            ok: false
        })
    property string message: ""
    property string packageQuery: ""
    property var packageRows: []
    property string packageMessage: ""
    property string packageWanted: ""
    property var ownership: ({})
    property string ownershipWanted: ""
    property bool refreshing: stateWorker.running
    readonly property var rows: NacreI18n.localize(catalog.rows)
    readonly property var navigation: Model.rail(rows)
    readonly property var softwareDesktopMetadata: new Map(softwareRows("software.discover").filter(row => row.desktopId).map(row => [row.desktopId, row]))
    readonly property var tuiDesktopMetadata: new Map((library.data.tui?.rows || []).map(entry => ["nacre-tui-" + entry.id, entry]))
    readonly property var apps: NacreApps.list.map(entry => {
        const row = NacreI18n.applicationRow(Model.application(entry));
        const terminal = tuiDesktopMetadata.get(entry.id.replace(/\.desktop$/, ""));
        if (terminal)
            return Object.assign(row, {
                tui: true,
                value: terminal.id,
                terminal: terminal.terminal
            });
        const metadata = softwareDesktopMetadata.get(entry.id);
        if (!metadata)
            return row;
        const extra = {};
        for (const key of ["software", "package", "desktopId", "categoryLabel", "website", "screenshots", "screenshotSource", "repository", "version", "checked", "status"])
            extra[key] = metadata[key];
        return Object.assign(row, extra);
    })
    readonly property var legacy: Legacy.actions(DesktopActions.list).map(entry => Object.assign({}, entry, {
            operation: "legacy-action",
            label: entry.name,
            breadcrumb: "Actions"
        }))
    readonly property var packageNames: new Set((snapshot.packages || []).map(entry => entry.name))
    readonly property string personality: NacreWallpapers.appearancePreferences.palettePersonality || (NacreWallpapers.appearancePreferences.paletteHarmony ? "harmony" : "natural")
    readonly property bool canAsk: !NacreAiViews.edge.busy && !NacreAiViews.edge.inWorkspace && !NacreAiViews.edge.draft.trim() && !NacreAiViews.edge.attachments.length

    function parse() {
        if (shared || !defaultText)
            return;
        try {
            Model.useTranslations(NacreI18n.tr);
            catalog = Model.merge(defaultText, extensionText);
        } catch (failure) {
            message = NacreI18n.tr("ui.menu-context.message.the-menu-catalog-could-not-be-loaded", "The menu catalog could not be loaded");
        }
    }
    readonly property Connections languageChanges: Connections {
        target: NacreI18n
        function onLanguageChanged() {
            root.parse();
        }
    }
    function refresh(force = false) {
        if (shared) {
            shared.refresh(force);
            return;
        }
        if (!stateWorker.running) {
            stateWorker.force = force;
            stateWorker.running = true;
        }
    }
    function refreshOnOpen() {
        refresh();
        workflows.refresh();
        library.refresh();
        DesktopExtras.request("clips", {});
        DesktopExtras.request("keys", {});
        NacreControlTools.refresh();
    }
    function prepare(route, query = "", opened = false) {
        // Reset/query bindings can request this more than once during an open.
        // Coalesce refreshes and let the panel start presenting first.
        if (opened)
            Qt.callLater(root.refreshOnOpen);
        const node = rows.find(item => item.id === route);
        if (node?.provider === "clipboard")
            DesktopExtras.request("clips", {});
        if (node?.provider === "keys")
            DesktopExtras.request("keys", {});
        if (route.startsWith("health"))
            Maintenance.refresh();
        const scope = Model.scope(query);
        previews.searchEmoji(scope.kind === "all" ? scope.text : "");
        const universal = scope.kind === "all" && scope.text.length >= 2;
        if (sourceOptions.files && (scope.kind === "files" || universal)) {
            fileWanted = scope.text;
            fileDelay.restart();
        }
        if (sourceOptions.calculator && scope.kind === "calculator" && /\b(to|in)\b/.test(scope.text)) {
            conversionWanted = scope.text;
            conversionDelay.restart();
        }
        if (sourceOptions.brain && (scope.kind === "brain" || universal)) {
            brainWanted = scope.text;
            brainDelay.restart();
        }
        if (!universal && scope.kind !== "files")
            fileWanted = "";
        if (!universal && scope.kind !== "brain")
            brainWanted = "";
        if (node?.provider === "packages") {
            packageWanted = scope.text;
            packageDelay.restart();
        }
    }
    function profileValue(value) {
        return ({
                "power-saver": PowerProfile.PowerSaver,
                balanced: PowerProfile.Balanced,
                performance: PowerProfile.Performance
            })[value];
    }
    readonly property bool recordingActive: workflows.data.ready === true ? workflows.data.recording === true : snapshot.recording === true
    function stateFor(row) {
        if (row.state === "power-profile")
            return PowerProfiles.profile === profileValue(row.value);
        if (row.state === "setting")
            return DesktopSettings.data[row.value] !== false;
        if (row.state === "personality")
            return personality === row.value;
        if (row.state === "palette-mode")
            return NacreWallpapers.appearancePreferences.paletteMode === row.value;
        if (row.state === "background")
            return !!NacreWallpapers.appearancePreferences.paletteBackgroundFromWallpaper === (row.value === "wallpaper");
        const states = {
            dnd: !!DesktopSettings.data.dnd,
            "night-light": !!NacreControlTools.data.nightLightEnabled,
            "stay-awake": !!snapshot.stayAwake,
            mute: NacreAudio.muted,
            "mic-mute": NacreAudio.micMuted,
            wifi: NacreNetwork.wifiEnabled,
            bluetooth: NacreBluetooth.powered
        };
        return states[row.state];
    }
    function decorate(row) {
        const result = Model.entry(row, rows);
        result.shortcut = DesktopExtras.keys.find(item => item.menu === row.id)?.key || "";
        const checked = stateFor(row);
        result.checked = checked === true;
        result.status = row.state === "updates" ? (Updates.count ? NacreI18n.tr("launcher.updates.available", {
                one: "{count} available",
                other: "{count} available"
            }, {
                count: Updates.count
            }) : NacreI18n.tr("ui.menu-context.status.up-to-date", "Up to date")) : typeof checked === "boolean" ? (["setting", "dnd", "night-light", "stay-awake", "mute", "mic-mute", "wifi", "bluetooth"].includes(row.state) ? (checked ? NacreI18n.tr("ui.menu-context.status.on", "On") : NacreI18n.tr("ui.menu-context.status.off", "Off")) : checked ? NacreI18n.tr("ui.menu-context.status.in-use", "In use") : "") : "";
        result.disabled = !!row.capability && snapshot.capabilities[row.capability] !== true && workflows.data.capabilities?.[row.capability] !== true;
        result.reason = result.disabled ? NacreI18n.tr("launcher.disabled.optional-tool-is-not-installed", "Optional tool is not installed") : "";
        if (row.id === "software.tui.new" && !(library.data.tui?.terminals || []).length) {
            result.disabled = true;
            result.reason = NacreI18n.tr("software.tui.requires-terminal", "Install a supported terminal first");
        }
        if (row.operation === "media") {
            const player = NacrePlayers.active;
            const capability = {
                playPause: "canTogglePlaying",
                next: "canGoNext",
                previous: "canGoPrevious"
            }[row.value];
            result.disabled = !player || !player[capability];
            result.reason = !player ? NacreI18n.tr("launcher.disabled.no-active-media-player", "No active media player") : result.disabled ? NacreI18n.tr("launcher.disabled.this-player-does-not-support-that-action", "This player does not support that action") : "";
        }
        if (row.value === "record-stop") {
            result.disabled = !recordingActive;
            result.reason = result.disabled ? NacreI18n.tr("launcher.disabled.no-recording-is-running", "No recording is running") : "";
        }
        if (["record-start", "record-screen"].includes(row.value) && recordingActive) {
            result.disabled = true;
            result.reason = NacreI18n.tr("launcher.disabled.a-recording-is-already-running", "A recording is already running");
        }
        if (row.value === "night-light") {
            result.disabled = !NacreControlTools.data.nightLightSupported || !!NacreControlTools.data.nightLightExternal;
            result.reason = NacreControlTools.data.nightLightExternal ? NacreI18n.tr("launcher.disabled.managed-by-an-existing-controller", "Managed by an existing controller") : result.disabled ? NacreI18n.tr("launcher.disabled.install-hyprsunset-to-use-night-light", "Install hyprsunset to use Night light") : "";
        }
        if (row.operation === "power-profile" && row.value === "performance" && !PowerProfiles.hasPerformanceProfile) {
            result.disabled = true;
            result.reason = NacreI18n.tr("launcher.disabled.performance-profile-is-unavailable-on-this-device", "Performance profile is unavailable on this device");
        }
        if (row.value === "mute")
            result.disabled = !NacreAudio.available;
        if (row.value === "mic-mute")
            result.disabled = !NacreAudio.micAvailable;
        if (row.operation === "toggle" && row.value === "wifi") {
            result.disabled = NacreNetwork.wifiAvailable === false;
            result.reason = result.disabled ? NacreI18n.tr("ui.menu-context.reason.no-wi-fi-adapter", "No Wi-Fi adapter") : "";
        }
        if (row.value === "bluetooth")
            result.disabled = !NacreBluetooth.available;
        return result;
    }
    function node(id) {
        const value = rows.find(row => row.id === id);
        return value ? decorate(value) : null;
    }
    function resolve(route) {
        return Model.resolve(route, rows);
    }
    function allStatic() {
        return rows.map(decorate);
    }
    function valueRow(parent, id, label, operation, value, icon, description = "", extra = {}) {
        return Object.assign({
            id: parent + "." + id,
            kind: "action",
            name: label,
            label: label,
            operation: operation,
            value: value,
            icon: icon,
            description: description,
            breadcrumb: Model.trail(parent, rows).map(row => row.label).join(" › "),
            disabled: false
        }, extra);
    }
    // Compositor snapshots can change geometry/focus without changing searchable
    // text. Compare the small search projection before rebuilding result rows.
    readonly property string windowSearchData: JSON.stringify((NacreCompositor.windows || []).map(client => ({
                windowId: client.windowId,
                title: client.title,
                wmClass: client.wmClass,
                workspaceName: client.workspace?.name
            })))
    readonly property var windowSearchRecords: JSON.parse(windowSearchData)
    // Other DesktopExtras replies also carry clipboard state. Equal content
    // should keep its rows stable instead of invalidating the global ranking.
    readonly property string clipboardSearchData: JSON.stringify(DesktopExtras.clips || [])
    readonly property var clipboardSearchRecords: JSON.parse(clipboardSearchData)
    function provider(route, query = "") {
        if (root.node(route)?.provider === "audio")
            return (NacreAudio.outputs || []).map(item => valueRow(route, "output-" + item.id, item.description || item.nickname || item.name, "audio-output", String(item.id), "speaker", NacreI18n.tr("launcher.provider.audio-output", "Audio output"), {
                    checked: NacreAudio.sink === item,
                    disabled: !item.ready
                }));
        const node = rows.find(row => row.id === route);
        if (!node?.provider)
            return [];
        if (node.provider === "apps")
            return apps;
        if (node.provider === "wallpapers")
            return NacreWallpapers.list.map(entry => valueRow(route, Qt.md5(entry.path), entry.name, "wallpaper", entry.path, "wallpaper", entry.dynamic ? NacreI18n.tr("launcher.provider.wallpaper.animated", "Animated wallpaper") : NacreI18n.tr("launcher.provider.wallpaper.static", "Wallpaper"), {
                    preview: entry.thumbnail || entry.poster,
                    checked: entry.path === NacreWallpapers.current,
                    status: entry.path === NacreWallpapers.current ? NacreI18n.tr("ui.menu-context.status.in-use", "In use") : ""
                }));
        if (node.provider === "accents")
            return NacreWallpapers.paletteOptions.map((entry, index) => valueRow(route, "choice-" + index, (entry.name ? NacreI18n.option("palette-name", entry.name) : "") || NacreI18n.tr("launcher.provider.color", "Color {number}", {
                    number: NacreI18n.number(index + 1)
                }), "accent", entry.accent, "palette", NacreI18n.tr("launcher.provider.use-this-color-family", "Use this color family"), {
                    colors: entry.colors || [],
                    checked: entry.accent === NacreWallpapers.selectedAccent,
                    status: entry.accent === NacreWallpapers.selectedAccent ? NacreI18n.tr("ui.menu-context.status.in-use", "In use") : ""
                }));
        if (node.provider === "fonts")
            return [valueRow(route, "default", NacreI18n.tr("launcher.provider.nacre-default", "Nacre default"), "font", "", "text_fields", NacreI18n.tr("ui.menu-context.description.ibm-plex-sans", "{font}", {
                    font: "IBM Plex Sans"
                }), {
                    checked: !DesktopSettings.data.interfaceFont
                }), ...(snapshot.fonts || []).map(name => valueRow(route, Qt.md5(name), name, "font", name, "text_fields", NacreI18n.tr("launcher.provider.preview-this-interface-font", "Preview this interface font"), {
                        previewFont: name,
                        disabled: name.length > 80,
                        checked: DesktopSettings.data.interfaceFont === name
                    }))];
        if (node.provider === "windows")
            return windowSearchRecords.map(client => valueRow(route, String(client.windowId), NacreI18n.windowCaption(client.title || client.wmClass, client.wmClass), "focus", client.windowId, "window", NacreI18n.tr("launcher.provider.workspace", "Workspace {workspace}", {
                    workspace: client.workspaceName
                })));
        if (node.provider === "wifi")
            return NacreNetwork.visibleNetworks.map(point => valueRow(route, point.bssid.replace(/:/g, ""), point.ssid || NacreI18n.tr("launcher.provider.hidden-network", "Hidden network"), "settings", "network", "wifi", NacreI18n.tr("launcher.provider.signal", "{strength}% signal", {
                    strength: NacreI18n.number(point.strength)
                }), {
                    checked: point.active,
                    status: point.active ? NacreI18n.tr("ui.menu-context.status.connected", "Connected") : ""
                }));
        if (node.provider === "bluetooth")
            return NacreBluetooth.devices.map(device => valueRow(route, device.address.replace(/:/g, ""), device.alias, "settings", "bluetooth", "bluetooth", device.paired ? NacreI18n.tr("launcher.provider.paired-device", "Paired device") : NacreI18n.tr("launcher.provider.nearby-device", "Nearby device"), {
                    checked: device.connected,
                    status: device.connected ? NacreI18n.tr("ui.menu-context.status.connected", "Connected") : ""
                }));
        if (node.provider === "clipboard")
            return clipboardSearchRecords.map(entry => valueRow(route, String(entry.id), entry.title || NacreI18n.tr("launcher.provider.clipboard-item", "Clipboard item"), "copy-clip", entry.id, "content_paste", NacreI18n.tr("launcher.provider.copied-item", "Copied item"), {
                    preview: entry.image ? entry.thumbnail || "" : "",
                    previewText: entry.image ? "" : entry.title || "",
                    clipboard: true,
                    pinned: entry.pinned === true,
                    checked: entry.pinned === true,
                    status: entry.pinned ? NacreI18n.tr("launcher.workflows.pinned", "Pinned") : ""
                }));
        if (node.provider === "keys")
            return DesktopExtras.keys.map((entry, index) => valueRow(route, "binding-" + index, entry.description || entry.key, "info", "shortcut", "keyboard", entry.key, {
                    shortcut: entry.key,
                    disabled: false
                }));
        if (node.provider === "emoji") {
            if (query)
                return previews.emojiQuery === query ? previews.emojiMatches.map(emojiRow) : [];
            const recent = LauncherPreferences.recent || [];
            return emojiRows.slice().sort((a, b) => {
                const favorite = Number(LauncherPreferences.favorites.includes(Model.stable(b))) - Number(LauncherPreferences.favorites.includes(Model.stable(a)));
                const ai = recent.indexOf(Model.stable(a)), bi = recent.indexOf(Model.stable(b));
                return favorite || (ai < 0 ? 999 : ai) - (bi < 0 ? 999 : bi);
            });
        }
        if (node.provider === "timers")
            return (snapshot.reminders || []).map(entry => valueRow(route, entry.unit.slice(0, -6), entry.label, "timer-cancel", entry.unit, "timer", NacreI18n.tr("launcher.provider.cancel-this-reminder", "Cancel this reminder"), {
                    status: NacreI18n.tr("ui.menu-context.status.until-value0", "Until {value0}", {
                        value0: NacreI18n.time(new Date(entry.due * 1000), "hh:mm")
                    })
                }));
        if (node.provider === "tui-apps")
            return tuiRows();
        if (node.provider === "webapps")
            return (snapshot.webapps || []).map(entry => valueRow(route, entry.id, entry.label, "webapp", entry.id, "public", entry.url, {
                    webapp: true
                }));
        if (node.provider === "installed")
            return (snapshot.packages || []).filter(entry => !query || Model.normalized(entry.name).includes(Model.normalized(query))).slice(0, 100).map(entry => valueRow(route, Qt.md5(entry.name), entry.name, "remove-package", entry.name, "inventory_2", entry.version, {
                    package: entry.name,
                    status: NacreI18n.tr("ui.menu-context.status.installed", "Installed")
                }));
        if (node.provider === "software-ai")
            return softwareRows(route).filter(row => row.category === "ai");
        if (node.provider === "software")
            return softwareRows(route);
        if (node.provider === "environments")
            return environmentRows();
        if (node.provider === "software-themes")
            return themeRows();
        if (node.provider === "software-extensions")
            return extensionRows();
        if (node.provider === "packages")
            return packageQuery === Model.scope(query).text ? packageRows.map(entry => valueRow(route, Qt.md5(entry.name), entry.name, "install-package", entry.name, "download", entry.description, {
                    package: entry.name,
                    status: packageNames.has(entry.name) ? NacreI18n.tr("ui.menu-context.status.installed", "Installed") : entry.repository,
                    checked: packageNames.has(entry.name),
                    disabled: packageNames.has(entry.name)
                })) : [];
        return [];
    }
    function emojiRow(entry) {
        const row = valueRow("actions.emoji", Qt.md5(entry.value), entry.name, "copy", entry.value, "mood", entry.category || NacreI18n.tr("launcher.provider.copy-this-emoji", "Copy this emoji"), {
            emoji: entry.value,
            aliases: [entry.value, ...(entry.aliases || [])]
        });
        row.searchIndex = Model.searchIndex(row);
        return row;
    }
    readonly property var emojiRows: shared ? shared.emojiRows : (emojiCatalog.length ? emojiCatalog : NacreI18n.localize(emojiData)).map(emojiRow)
    readonly property var emojiData: [
        {
            value: "😀",
            nameId: "ui.nacremenucontext.name.grinning-face",
            name: "Grinning face"
        },
        {
            value: "😂",
            nameId: "ui.nacremenucontext.name.laughing",
            name: "Laughing"
        },
        {
            value: "🥹",
            nameId: "ui.nacremenucontext.name.holding-back-tears",
            name: "Holding back tears"
        },
        {
            value: "😊",
            nameId: "ui.nacremenucontext.name.smiling",
            name: "Smiling"
        },
        {
            value: "😍",
            nameId: "ui.nacremenucontext.name.heart-eyes",
            name: "Heart eyes"
        },
        {
            value: "🤔",
            nameId: "ui.nacremenucontext.name.thinking",
            name: "Thinking"
        },
        {
            value: "😎",
            nameId: "ui.nacremenucontext.name.cool",
            name: "Cool"
        },
        {
            value: "😭",
            nameId: "ui.nacremenucontext.name.crying",
            name: "Crying"
        },
        {
            value: "😅",
            nameId: "ui.nacremenucontext.name.nervous-laugh",
            name: "Nervous laugh"
        },
        {
            value: "😴",
            nameId: "ui.nacremenucontext.name.sleeping",
            name: "Sleeping"
        },
        {
            value: "👍",
            nameId: "ui.nacremenucontext.name.thumbs-up",
            name: "Thumbs up"
        },
        {
            value: "👎",
            nameId: "ui.nacremenucontext.name.thumbs-down",
            name: "Thumbs down"
        },
        {
            value: "👏",
            nameId: "ui.nacremenucontext.name.clapping",
            name: "Clapping"
        },
        {
            value: "🙏",
            nameId: "ui.nacremenucontext.name.thank-you",
            name: "Thank you"
        },
        {
            value: "💪",
            nameId: "ui.nacremenucontext.name.strong",
            name: "Strong"
        },
        {
            value: "👋",
            nameId: "ui.nacremenucontext.name.wave",
            name: "Wave"
        },
        {
            value: "❤️",
            nameId: "ui.nacremenucontext.name.heart",
            name: "Heart"
        },
        {
            value: "💜",
            nameId: "ui.nacremenucontext.name.purple-heart",
            name: "Purple heart"
        },
        {
            value: "💙",
            nameId: "ui.nacremenucontext.name.blue-heart",
            name: "Blue heart"
        },
        {
            value: "💚",
            nameId: "ui.nacremenucontext.name.green-heart",
            name: "Green heart"
        },
        {
            value: "🔥",
            nameId: "ui.nacremenucontext.name.fire",
            name: "Fire"
        },
        {
            value: "✨",
            nameId: "ui.nacremenucontext.name.sparkles",
            name: "Sparkles"
        },
        {
            value: "🎉",
            nameId: "ui.nacremenucontext.name.celebrate",
            name: "Celebrate"
        },
        {
            value: "✅",
            nameId: "ui.nacremenucontext.name.done",
            name: "Done"
        },
        {
            value: "❌",
            nameId: "ui.nacremenucontext.name.no",
            name: "No"
        },
        {
            value: "⚠️",
            nameId: "ui.nacremenucontext.name.warning",
            name: "Warning"
        },
        {
            value: "🚀",
            nameId: "ui.nacremenucontext.name.rocket",
            name: "Rocket"
        },
        {
            value: "🐚",
            nameId: "ui.nacremenucontext.name.shell",
            name: "Shell"
        },
        {
            value: "🫧",
            nameId: "ui.nacremenucontext.name.bubbles",
            name: "Bubbles"
        },
        {
            value: "🌊",
            nameId: "ui.nacremenucontext.name.wave-ocean",
            name: "Wave ocean"
        },
        {
            value: "☕",
            nameId: "ui.nacremenucontext.name.coffee",
            name: "Coffee"
        },
        {
            value: "💻",
            nameId: "ui.nacremenucontext.name.laptop",
            name: "Laptop"
        },
        {
            value: "🧠",
            nameId: "ui.nacremenucontext.name.brain",
            name: "Brain"
        },
        {
            value: "🌙",
            nameId: "ui.nacremenucontext.name.moon",
            name: "Moon"
        },
        {
            value: "🌈",
            nameId: "ui.nacremenucontext.name.rainbow",
            name: "Rainbow"
        },
        {
            value: "💡",
            nameId: "ui.nacremenucontext.name.idea",
            name: "Idea"
        },
        {
            value: "🛠️",
            nameId: "ui.nacremenucontext.name.tools",
            name: "Tools"
        },
        {
            value: "📌",
            nameId: "ui.nacremenucontext.name.pinned",
            name: "Pinned"
        },
        {
            value: "📷",
            nameId: "ui.nacremenucontext.name.camera",
            name: "Camera"
        },
        {
            value: "🎵",
            nameId: "ui.nacremenucontext.name.music",
            name: "Music"
        }
    ]
    function favorites() {
        const needed = rows.filter(row => row.provider && LauncherPreferences.favorites.some(id => id.startsWith("nacre-menu:" + row.id + ".")));
        let candidates = allStatic().concat(workflowRows());
        for (const row of needed)
            candidates = candidates.concat(provider(row.id));
        return Model.alphabetical(LauncherPreferences.favorites.map(id => Model.favorite(id, apps, candidates, legacy)).filter(Boolean));
    }
    function recent() {
        const candidates = allStatic().concat(workflowRows());
        return LauncherPreferences.recent.map(id => Model.favorite(id, apps, candidates, legacy)).filter(row => row && !keepOpen(row)).slice(0, 5);
    }
    function brainRows(query) {
        if (brainQuery !== query)
            return [];
        return notes.map(entry => valueRow("actions.assistant.search", Qt.md5(entry.path), entry.title, "note", entry.path, "description", entry.preview, {
                breadcrumb: "Nacre Brain",
                note: true,
                matchedQuery: query,
                filePath: (workflows.data.brainRoot || "") + "/" + entry.path,
                previewText: entry.preview
            }));
    }
    readonly property var searchFilters: [
        {
            value: "all",
            label: NacreI18n.tr("launcher.filters.all", "Everything")
        },
        {
            value: "apps",
            label: NacreI18n.tr("launcher.filters.apps", "Apps")
        },
        {
            value: "files",
            label: NacreI18n.tr("launcher.filters.files", "Files")
        },
        {
            value: "windows",
            label: NacreI18n.tr("launcher.filters.windows", "Windows")
        },
        {
            value: "clipboard",
            label: NacreI18n.tr("launcher.filters.clipboard", "Clipboard")
        },
        {
            value: "notes",
            label: NacreI18n.tr("launcher.filters.notes", "Notes")
        },
        {
            value: "emoji",
            label: NacreI18n.tr("launcher.filters.emoji", "Emoji")
        },
        {
            value: "saved",
            label: NacreI18n.tr("launcher.filters.saved", "Saved items")
        }
    ]
    function resultCategory(entry) {
        if (entry.kind === "app")
            return "apps";
        if (entry.emoji)
            return "emoji";
        if (entry.note)
            return "notes";
        if (entry.clipboard)
            return "clipboard";
        if (entry.file)
            return "files";
        if (entry.operation === "focus")
            return "windows";
        if (entry.operation === "workflow")
            return "saved";
        return "actions";
    }
    function fileResults(query) {
        return fileQuery === query ? fileRows.map(item => valueRow("actions.files", item.id, item.name, "open-file", item.path, "description", item.path, {
                file: true
            })) : [];
    }
    function workflowRows(query = "") {
        const saved = (workflows.data.entries || []).map(item => valueRow("actions.saved", item.id, item.label, "workflow", item.id, item.icon || ({
                    action: "terminal",
                    snippet: "content_paste",
                    search: "public",
                    bookmark: "bookmark",
                    workspace: "workspaces"
                })[item.type], item.type === "snippet" ? item.text : item.directory || item.url || "", {
                workflow: item,
                searchQuery: query,
                previewText: item.type === "snippet" ? item.text : ""
            }));
        const projects = (workflows.data.projects || []).map(item => valueRow("actions.saved", "project-" + item.id, item.label || item.id, "workflow", "project:" + item.id, "workspaces", item.path, {
                searchQuery: query
            }));
        return saved.concat(projects);
    }
    function captureRows() {
        return (workflows.data.captures || []).map(item => valueRow("actions.capture", Qt.md5(item.path), item.name, "open-file", item.path, "perm_media", item.path, {
                file: true,
                capture: true
            }));
    }
    function webSearchRows(query) {
        return (workflows.data.entries || []).filter(item => item.type === "search").map(item => valueRow("actions.saved", item.id, NacreI18n.tr("launcher.workflows.search-engine", "Search {engine} for “{query}”", {
                engine: item.label,
                query: query.slice(0, 100)
            }), "workflow", item.id, "public", NacreI18n.tr("ui.menu-context.description.opens-your-browser-only-when-selected", "Opens your browser only when selected"), {
                workflow: item,
                searchQuery: query
            }));
    }
    readonly property var softwareCategories: [
        {
            value: "all",
            label: NacreI18n.tr("software.category.all", "All apps")
        },
        {
            value: "ai",
            label: NacreI18n.tr("software.category.ai", "AI")
        },
        {
            value: "browsers",
            label: NacreI18n.tr("software.category.browsers", "Browsers")
        },
        {
            value: "communication",
            label: NacreI18n.tr("software.category.communication", "Communication")
        },
        {
            value: "games",
            label: NacreI18n.tr("software.category.games", "Games")
        },
        {
            value: "creative",
            label: NacreI18n.tr("software.category.creative", "Creative")
        },
        {
            value: "office",
            label: NacreI18n.tr("software.category.office", "Office and reading")
        },
        {
            value: "development",
            label: NacreI18n.tr("software.category.development", "Development")
        },
        {
            value: "network",
            label: NacreI18n.tr("software.category.network", "Network")
        },
        {
            value: "utilities",
            label: NacreI18n.tr("software.category.utilities", "Utilities")
        }
    ]
    readonly property var softwareAppIndex: {
        const names = new Set(), ids = new Set(), visible = new Map();
        for (const app of NacreApps.all || []) {
            names.add(Model.normalized(app.name));
            ids.add(Model.normalized(app.id.replace(/\.desktop$/, "")));
            ids.add(Model.normalized(app.id.replace(/\.desktop$/, "").split(".").pop()));
        }
        for (const app of NacreApps.list || []) {
            visible.set(Model.normalized(app.name), app.id);
            visible.set(Model.normalized(app.id.replace(/\.desktop$/, "")), app.id);
            visible.set(Model.normalized(app.id.replace(/\.desktop$/, "").split(".").pop()), app.id);
        }
        return {
            names: names,
            ids: ids,
            visible: visible
        };
    }
    function softwareRows(route) {
        const index = softwareAppIndex;
        return NacreI18n.localize(catalog.software).map(entry => {
            const packageId = Model.normalized(entry.package.replace(/-(bin|git)$/, ""));
            const appId = Model.normalized(entry.appId || "");
            const name = Model.normalized(entry.label);
            const installedPackage = [entry.package, ...(entry.packageAlternatives || [])].find(name => packageNames.has(name));
            const installed = !!installedPackage || index.names.has(name) || index.ids.has(packageId) || !!appId && index.ids.has(appId);
            const desktopId = index.visible.get(name) || index.visible.get(appId) || index.visible.get(packageId) || "";
            const aurMissing = entry.repository === "AUR" && !snapshot.capabilities.aur;
            const savedWeb = (snapshot.webapps || []).find(item => item.url.replace(/\/$/, "") === entry.webUrl?.replace(/\/$/, ""));
            const packageAvailable = entry.repository !== "OpenAI" || library.data.ai?.chatgptPackage === true;
            if (entry.webUrl && !installedPackage && (savedWeb || !installed && (aurMissing || !snapshot.capabilities.packages || !packageAvailable)))
                return valueRow(route, Qt.md5(entry.package), entry.label, savedWeb ? "webapp" : "webapp-offer", savedWeb?.id || entry.webUrl, "public", entry.description, {
                    software: true,
                    category: "ai",
                    categoryLabel: NacreI18n.tr("software.category.ai", "AI"),
                    webUrl: entry.webUrl,
                    webapp: !!savedWeb,
                    desktopId: index.visible.get(Model.normalized(entry.label)) || "",
                    checked: !!savedWeb,
                    website: entry.website,
                    aliases: [entry.package, ...(entry.aliases || [])],
                    status: savedWeb ? NacreI18n.tr("software.ai.web-installed", "Web app installed") : NacreI18n.tr("software.ai.web-offer", "Create web app"),
                    disabled: !savedWeb && library.data.ai?.webApps !== true,
                    reason: NacreI18n.tr("software.ai.web-requires-browser", "Requires a Chromium-based browser for the web app")
                });

            return valueRow(route, Qt.md5(entry.package), entry.label, installed && entry.terminalCommand ? "ai-terminal" : installed ? desktopId ? "software-open" : "info" : "install-package", installedPackage || entry.package, "download", entry.description, {
                package: installedPackage || entry.package,
                terminalCommand: entry.terminalCommand || [],
                software: true,
                category: entry.category || "utilities",
                categoryLabel: softwareCategories.find(row => row.value === entry.category)?.label || "",
                version: (snapshot.packages || []).find(row => row.name === entry.package)?.version || "",
                desktopId: desktopId,
                website: entry.website || "",
                screenshots: entry.screenshots || [],
                screenshotSource: entry.screenshotSource || "",
                repository: entry.repository || "",
                aliases: [entry.package, ...(entry.aliases || [])],
                checked: installed,
                status: installed ? NacreI18n.tr("ui.menu-context.status.installed", "Installed") : NacreI18n.tr("ui.menu-context.status.install", "Install"),
                disabled: installed && entry.terminalCommand ? library.data.ai?.terminal !== true : !installed && (!snapshot.capabilities.packages || aurMissing),
                reason: installed && !desktopId ? NacreI18n.tr("launcher.disabled.installed-use-item-actions-to-remove-it", "Installed; use item actions to remove it") : !installed && aurMissing ? NacreI18n.tr("launcher.install-offer.aur", "Requires paru or yay for AUR installation") : !installed && !snapshot.capabilities.packages ? NacreI18n.tr("launcher.disabled.requires-an-arch-compatible-package-manager", "Requires an Arch-compatible package manager") : ""
            });
        });
    }
    function installOffers() {
        return softwareRows("software.discover").filter(row => !row.checked).map(row => Object.assign({}, row, {
                installOffer: true,
                description: NacreI18n.tr("launcher.install-offer.description", "Install {app} · {purpose}", {
                    app: row.name,
                    purpose: row.description
                })
            }));
    }
    function themeRows() {
        return (library.data.themes || NacreWallpapers.palettes || []).map(theme => valueRow("software.themes", Qt.md5(theme.id), theme.name, "software-theme", theme.id, "palette", NacreI18n.tr("software.theme.description", "Use this palette throughout your desktop and applications"), {
                theme: true,
                colors: theme.swatches || [theme.modes?.dark?.primary, theme.modes?.dark?.secondary, theme.modes?.dark?.tertiary].filter(Boolean),
                themeModes: theme.modes || ({}),
                surface: theme.surface || theme.modes?.dark?.surface || "#151515",
                group: theme.group || "",
                checked: NacreWallpapers.appearancePreferences.palettePreset === theme.id,
                status: NacreWallpapers.appearancePreferences.palettePreset === theme.id ? NacreI18n.tr("ui.menu-context.status.in-use", "In use") : "",
                aliases: [NacreI18n.tr("software.theme.alias", "theme colors palette")]
            }));
    }
    function extensionRows() {
        return (library.data.extensions || []).map(item => valueRow("software.extensions", Qt.md5(item.id), item.name, item.available ? "software-extension-install" : "software-extension-toggle", item.id, "extension", item.description, {
                extension: item,
                version: item.version || "",
                website: item.website || "",
                preview: item.previewPath || "",
                checked: item.enabled === true,
                status: item.available ? NacreI18n.tr("software.extension.available", "Available") : item.enabled ? NacreI18n.tr("software.extension.enabled", "Enabled") : NacreI18n.tr("software.extension.disabled", "Disabled"),
                disabled: item.broken === true,
                reason: item.broken ? item.description : "",
                aliases: [item.id, ...(item.kinds || [])]
            }));
    }
    function extensionActions() {
        const result = [];
        for (const extension of library.data.extensions || []) {
            if (!extension.enabled || extension.broken)
                continue;
            for (const action of extension.actions || [])
                result.push(valueRow("software.extensions", Qt.md5(extension.id + ":" + action.id), action.name, "software-extension-action", extension.id, action.icon || "extension", action.description || extension.name, {
                    extension: extension,
                    extensionAction: action.id
                }));
            if ((extension.kinds || []).includes("panel"))
                result.push(valueRow("software.extensions", Qt.md5(extension.id + ":panel"), extension.name, "software-extension-open", extension.id, "extension", extension.description, {
                    extension: extension
                }));
        }
        return result;
    }
    function environmentRows() {
        const result = [];
        for (const profile of library.data.environments?.profiles || []) {
            for (const operation of ["install", "remove"]) {
                const installed = profile.installed === true;
                const name = operation === "install" ? NacreI18n.tr("software.development.install", "Install {name}", {
                    name: profile.name
                }) : NacreI18n.tr("software.development.remove", "Remove {name}", {
                    name: profile.name
                });
                result.push(valueRow("software.development", profile.id + "-" + operation, name, "environment", profile.id, profile.kind === "database" ? "database" : "code", profile.kind === "database" ? NacreI18n.tr("software.development.database-description", "Local Docker database; removal retains its data") : NacreI18n.tr("software.development.mise-description", "Managed by mise; runs in a visible terminal"), {
                    environmentOperation: operation,
                    disabled: !profile.available || operation === "remove" && !installed,
                    reason: profile.reason || (operation === "remove" && !installed ? NacreI18n.tr("software.development.not-installed", "Not installed") : ""),
                    aliases: [profile.name, profile.id, "development runtime mise"],
                    confirm: NacreI18n.tr("software.development.open-terminal", "Open a terminal to review and confirm this change?")
                }));
            }
        }
        return result;
    }
    function tuiRows() {
        if (library.data.tui?.error)
            return [valueRow("software.tui", "error", NacreI18n.tr("software.tui.registry-error", "Terminal apps need attention"), "info", "tui", "terminal", library.data.tui.error, {
                    disabled: true
                })];
        return (library.data.tui?.rows || []).map(entry => valueRow("software.tui", entry.id, entry.name, "tui-launch", entry.id, entry.icon, entry.command.join(" · "), {
                tui: true,
                terminal: entry.terminal,
                disabled: !entry.available,
                reason: entry.available ? "" : NacreI18n.tr("software.tui.requires-command", "Install the selected terminal and command first")
            }));
    }
    function installedAiRows() {
        return softwareRows("software.discover.ai").filter(row => row.category === "ai" && row.checked && !row.desktopId);
    }
    function globalRows(query) {
        return apps.concat(allStatic().filter(row => sourceOptions.actions || row.kind === "group" || row.id.startsWith("software.")), provider("actions.windows"), provider("actions.clipboard"), sourceOptions.files ? fileResults(query) : [], sourceOptions.brain ? brainRows(query) : [], workflowRows(query), provider("actions.emoji", query), provider("style.wallpapers"), provider("style.fonts"), installOffers(), themeRows(), extensionRows().filter(row => !row.extension.enabled), extensionActions(), environmentRows(), installedAiRows(), tuiRows().filter(row => !apps.some(app => app.id.replace(/\.desktop$/, "") === "nacre-tui-" + row.value)), captureRows());
    }
    function itemActions(entry) {
        if (!entry)
            return [];
        const result = [];
        const add = function (name, operation, value, extra) {
            const row = Object.assign({
                id: entry.id + "." + operation,
                kind: "action",
                name: name,
                operation: operation,
                value: value === undefined ? entry.filePath || entry.value : value,
                transient: true
            }, extra || {});
            if (operation === "file-action") {
                row.capability = ({
                        share: "share",
                        annotate: "annotate",
                        compress: "convert",
                        gif: "convert",
                        "image-png": "convert",
                        "image-jpeg": "convert"
                    })[row.action] || "";
                if (row.capability && workflows.data.capabilities && workflows.data.capabilities[row.capability] !== true) {
                    row.disabled = true;
                    row.reason = NacreI18n.tr("launcher.workflows.install-tool", "Install the optional tool to use this action");
                }
            }
            result.push(row);
        };
        if (entry.website)
            add(NacreI18n.tr("software.action.website", "Project website"), "url", entry.website);
        if (entry.software && entry.checked) {
            if (entry.desktopId)
                add(NacreI18n.tr("software.action.open", "Open app"), "software-open", entry.package);
            add(NacreI18n.tr("software.action.updates", "Check system updates"), "tool", "updates");
            if (packageNames.has(entry.package))
                add(NacreI18n.tr("software.action.remove", "Remove app"), "remove-package", entry.package, {
                    confirm: NacreI18n.tr("software.action.remove-confirm", "Open the package manager to remove this app?")
                });
        }
        if (entry.tui)
            add(NacreI18n.tr("software.tui.remove", "Remove terminal app"), "tui-remove", entry.value, {
                confirm: NacreI18n.tr("software.tui.remove-confirm", "Remove this launcher entry? The command and its data will be retained.")
            });
        if (entry.theme || entry.screenshots?.length)
            add(NacreI18n.tr("software.preview.more", "View previews"), "software-preview");
        if (entry.theme) {
            add(NacreI18n.tr("software.theme.apply-dark", "Apply dark theme"), "software-theme", entry.value, {
                themeMode: "dark"
            });
            add(NacreI18n.tr("software.theme.apply-light", "Apply light theme"), "software-theme", entry.value, {
                themeMode: "light"
            });
            add(NacreI18n.tr("software.theme.export", "Export theme"), "form", "software-theme-export", {
                themePreset: entry.value
            });
            if (entry.value.startsWith("favorite:") && !entry.checked)
                add(NacreI18n.tr("software.theme.remove", "Remove saved theme"), "software-theme-remove", entry.value, {
                    confirm: NacreI18n.tr("software.theme.remove-confirm", "Remove this saved palette? Your other themes will be kept.")
                });
        }
        if (entry.extension && !entry.extension.available) {
            add(entry.extension.enabled ? NacreI18n.tr("software.action.disable", "Disable extension") : NacreI18n.tr("software.action.enable", "Enable extension"), "software-extension-toggle");
            if ((entry.extension.kinds || []).includes("panel") && entry.extension.enabled)
                add(NacreI18n.tr("software.action.open-panel", "Open panel"), "software-extension-open");
            if ((entry.extension.kinds || []).includes("theme"))
                add(NacreI18n.tr("software.action.install-theme", "Add theme to Themes"), "software-extension-theme");
            add(NacreI18n.tr("software.action.open-folder", "Open extension folder"), "software-extension-folder");
            if ((entry.extension.source || "").startsWith("https://"))
                add(NacreI18n.tr("software.action.update", "Update extension"), "software-extension-update", entry.value, {
                    confirm: NacreI18n.tr("software.action.update-confirm", "Download the current version? It will be disabled until you enable it again.")
                });
            add(NacreI18n.tr("software.action.clone", "Clone to edit"), "form", "software-clone", {
                sourceExtension: entry.value
            });
            add(NacreI18n.tr("software.action.remove-extension", "Remove extension"), "software-extension-remove", entry.value, {
                confirm: NacreI18n.tr("software.action.remove-extension-confirm", "Remove this extension and its managed files?")
            });
        }
        if (entry.kind === "app") {
            const key = String(entry.id).replace(/\.desktop$/, "").toLowerCase();
            const keys = [key, key.split(".").pop()];
            for (const client of NacreCompositor.windows || []) {
                if (keys.includes(String(client.wmClass || "").toLowerCase()))
                    add(NacreI18n.tr("launcher.workflows.switch-window", "Switch to {title}", {
                        title: client.title || entry.name
                    }), "focus", client.windowId);
            }
        }
        if (entry.clipboard) {
            add(entry.pinned ? NacreI18n.tr("launcher.workflows.unpin", "Unpin clipboard item") : NacreI18n.tr("launcher.workflows.pin", "Pin clipboard item"), "clip-pin");
            add(NacreI18n.tr("launcher.workflows.delete-clip", "Delete clipboard item"), "clip-delete");
        }
        if (entry.file || entry.note) {
            add(NacreI18n.tr("launcher.workflows.reveal", "Show folder"), "file-action", entry.filePath || entry.value, {
                action: "reveal"
            });
            add(NacreI18n.tr("launcher.workflows.copy-path", "Copy path"), "copy");
            add(NacreI18n.tr("launcher.workflows.copy-file", "Copy file"), "file-action", entry.filePath || entry.value, {
                action: "copy-file"
            });
            add(NacreI18n.tr("launcher.workflows.share-file", "Share with LocalSend"), "file-action", entry.filePath || entry.value, {
                action: "share"
            });
            if (/\.(png|jpe?g|webp|bmp)$/i.test(entry.value)) {
                add(NacreI18n.tr("launcher.workflows.annotate", "Annotate image"), "file-action", entry.filePath || entry.value, {
                    action: "annotate"
                });
                add(NacreI18n.tr("launcher.workflows.png", "Convert to PNG"), "file-action", entry.filePath || entry.value, {
                    action: "image-png"
                });
                add(NacreI18n.tr("launcher.workflows.jpeg", "Convert to JPEG"), "file-action", entry.filePath || entry.value, {
                    action: "image-jpeg"
                });
            }
            if (/\.(mp4|mkv|webm|mov)$/i.test(entry.value)) {
                add(NacreI18n.tr("launcher.workflows.compress", "Compress video"), "file-action", entry.filePath || entry.value, {
                    action: "compress"
                });
                add(NacreI18n.tr("launcher.workflows.gif", "Create GIF"), "file-action", entry.filePath || entry.value, {
                    action: "gif"
                });
            }
        }
        if (entry.workflow)
            add(NacreI18n.tr("launcher.workflows.remove", "Remove saved item"), "workflow-remove");
        return result;
    }
    function ranked(items, query) {
        return Model.ranked(items, query, LauncherPreferences.usage || {}, LauncherPreferences.recent);
    }
    function content(route, query, filter = "all", softwareCategory = "all") {
        const scope = Model.scope(query);
        if (["brain", "files", "calculator", "actions", "ai"].includes(scope.kind) && sourceOptions[scope.kind] === false)
            return [valueRow("setup", "source-disabled", NacreI18n.tr("launcher.sources.disabled", "This search source is turned off in Settings"), "info", "sources", "settings", NacreI18n.tr("launcher.sources.disabled-description", "Enable it in Settings to use this prefix"), {
                    disabled: true
                })];
        if (scope.kind === "files")
            return fileResults(scope.text);
        if (scope.kind === "calculator")
            return [];
        if (scope.kind === "brain")
            return brainRows(scope.text);
        if (scope.kind === "ai")
            return scope.text ? [valueRow("actions.assistant", "question", NacreI18n.tr("ui.menu-context.label.ask-nacre-ai", "Ask Nacre AI"), "ask", scope.text, "chat", scope.text, {
                    brand: "ai",
                    disabled: !canAsk || !snapshot.capabilities.ai,
                    reason: !snapshot.capabilities.ai ? NacreI18n.tr("launcher.disabled.nacre-ai-is-optional-open-setup-to-add-it", "Nacre AI is optional; open Setup to add it") : !canAsk ? NacreI18n.tr("launcher.disabled.an-assistant-task-or-unsent-draft-is-active-open-nacre-ai-instead", "An assistant task or unsent draft is active; open Nacre AI instead") : ""
                })] : [];
        if (scope.kind === "help")
            return [
                {
                    id: "scope-actions",
                    kind: "action",
                    name: NacreI18n.tr("ui.menu-context.name.actions", "> Actions"),
                    label: NacreI18n.tr("ui.menu-context.label.actions", "> Actions"),
                    description: NacreI18n.tr("ui.menu-context.description.search-desktop-commands", "Search desktop commands"),
                    operation: "scope",
                    value: "> ",
                    icon: "bolt"
                },
                {
                    id: "scope-calculator",
                    kind: "action",
                    name: NacreI18n.tr("ui.menu-context.name.calculator", "= Calculator"),
                    label: NacreI18n.tr("ui.menu-context.label.calculator", "= Calculator"),
                    description: NacreI18n.tr("ui.menu-context.description.calculate-and-copy-a-result", "Calculate and copy a result"),
                    operation: "scope",
                    value: "= ",
                    icon: "calculate"
                },
                {
                    id: "scope-brain",
                    kind: "action",
                    name: NacreI18n.tr("ui.menu-context.name.brain", "# Brain"),
                    label: NacreI18n.tr("ui.menu-context.label.brain", "# Brain"),
                    description: NacreI18n.tr("ui.menu-context.description.search-private-notes", "Search private notes"),
                    operation: "scope",
                    value: "# ",
                    icon: "neurology"
                },
                {
                    id: "scope-files",
                    kind: "action",
                    name: NacreI18n.tr("ui.menu-context.name.files", ": Files"),
                    label: NacreI18n.tr("ui.menu-context.label.files", ": Files"),
                    description: NacreI18n.tr("ui.menu-context.description.search-your-usual-folders", "Search your usual folders"),
                    operation: "scope",
                    value: ": ",
                    icon: "folder"
                },
                {
                    id: "scope-ai",
                    kind: "action",
                    name: NacreI18n.tr("ui.menu-context.name.nacre-ai", "? Nacre AI"),
                    label: NacreI18n.tr("ui.menu-context.label.nacre-ai", "? Nacre AI"),
                    description: NacreI18n.tr("ui.menu-context.description.ask-an-explicit-question", "Ask an explicit question"),
                    operation: "scope",
                    value: "? ",
                    icon: "chat"
                }
            ].filter(row => sourceOptions[row.id.replace("scope-", "")] !== false);
        if (scope.kind === "actions")
            return ranked(allStatic().filter(row => row.kind !== "group"), scope.text);
        if (scope.text) {
            return ranked(globalRows(scope.text).concat(route === "software.packages" ? provider(route, scope.text) : []).filter(row => filter === "all" || resultCategory(row) === filter), scope.text).concat(filter === "all" ? webSearchRows(scope.text) : [], filter === "all" ? [valueRow("actions", "search-web", NacreI18n.tr("ui.menu-context.label.search-the-web-for-value0", "Search the web for “{value0}”", {
                    value0: scope.text.slice(0, 100)
                }), "search-web", scope.text, "public", NacreI18n.tr("ui.menu-context.description.opens-your-browser-only-when-selected", "Opens your browser only when selected"))] : []);
        }
        if (route === "home")
            return favorites();
        if (route === "apps")
            return apps;
        if (route === "hidden")
            return NacreApps.all.filter(entry => LauncherPreferences.hidden.includes(entry.id)).map(entry => Object.assign(Model.application(entry), {
                    hidden: true
                }));
        if (route === "software.discover" && softwareCategory !== "all")
            return provider(route).filter(row => row.category === softwareCategory);
        if (route === "actions.saved")
            return Model.children(route, rows).map(decorate).concat(workflowRows());
        if (route === "actions.capture")
            return Model.children(route, rows).map(decorate).concat(captureRows());
        return Model.children(route, rows).map(decorate).concat(provider(route));
    }
    function record(row) {
        if (!row.transient && !keepOpen(row) && row.operation !== "ask" && row.kind !== "calculation" && row.operation !== "scope" && row.operation !== "info")
            LauncherPreferences.update("record", Model.stable(row), true);
    }
    function keepOpen(row) {
        return ["toggle", "setting-toggle", "personality", "background", "palette-mode", "accent", "wallpaper", "next-wallpaper", "font", "info", "scope", "form", "timer-cancel", "webapp-remove", "power-profile", "media", "audio-output", "clip-pin", "clip-delete", "file-action", "workflow-remove", "software-theme", "software-extension-toggle", "software-extension-install", "software-extension-update", "software-extension-remove", "software-extension-theme"].includes(row.operation);
    }
    function execute(row, confirmed = false) {
        if (shared)
            return shared.execute(row, confirmed);
        if (!row || row.disabled || row.kind === "group")
            return false;
        if ((row.confirm || row.operation === "power" && row.value !== "lock" || row.operation === "maintenance" && row.value === "restart") && !confirmed)
            return false;
        if (row.operation === "tool" && row.value === "record-stop")
            return workflows.work("record-stop");
        if (row.operation === "workflow")
            return workflows.work("activate", row.value, {
                query: row.searchQuery || ""
            });
        if (row.operation === "workflow-remove")
            return workflows.work("remove", row.value);
        if (row.operation === "file-action")
            return workflows.work(row.action, row.value);
        if (["clip-pin", "clip-delete"].includes(row.operation)) {
            DesktopExtras.request(row.operation === "clip-pin" ? "pin" : "delete", {
                id: row.value
            });
            return true;
        }
        if (row.operation === "media")
            return NacrePlayers.control(row.value);
        if (row.operation === "audio-output")
            return NacreAudio.chooseOutput(row.value);
        if (row.operation === "open-file") {
            AppLaunch.run(["xdg-open", row.value]);
            return true;
        }
        if (row.operation === "search-web") {
            AppLaunch.run(["nacre-app", "browser", Sources.webUrl(sourceOptions.webEngine, row.value)]);
            return true;
        }
        record(row);
        const operation = row.operation, value = row.value;
        if (operation === "app")
            return NacreApps.launch(row.app);
        if (operation === "legacy-action") {
            DesktopActions.execute(row.action, row.value);
            return true;
        }
        if (operation === "legacy") {
            DesktopActions.execute(value, "");
            return true;
        }
        if (operation === "settings") {
            NacrePanelState.openSettings(value);
            return true;
        }
        if (operation === "personality") {
            NacreWallpapers.preference({
                palettePersonality: value
            });
            return true;
        }
        if (operation === "background") {
            NacreWallpapers.preference({
                paletteBackgroundFromWallpaper: value === "wallpaper"
            });
            return true;
        }
        if (operation === "palette-mode")
            return NacreColours.setMode(value);
        if (operation === "accent") {
            NacreWallpapers.preference({
                paletteAccent: value
            });
            return true;
        }
        if (operation === "wallpaper") {
            NacreWallpapers.setWallpaper(value);
            return true;
        }
        if (operation === "next-wallpaper") {
            const list = NacreWallpapers.list;
            if (list.length)
                NacreWallpapers.setWallpaper(list[(list.findIndex(entry => entry.path === NacreWallpapers.current) + 1) % list.length].path);
            return !!list.length;
        }
        if (operation === "power-profile" && profileValue(value) !== undefined) {
            PowerProfiles.profile = profileValue(value);
            return true;
        }
        if (operation === "tui-launch" || operation === "tui-remove")
            return library.work(operation, value);
        if (operation === "webapp-offer") {
            if (!library.data.ai?.webApps)
                return false;
            work("webapp-create", "", {
                label: row.name,
                url: row.webUrl
            });
            return true;
        }
        if (operation === "ai-terminal") {
            if (!library.data.ai?.terminal || !Array.isArray(row.terminalCommand))
                return false;
            AppLaunch.run(["kitty", "--hold", "--", ...row.terminalCommand]);
            return true;
        }
        if (operation === "environment") {
            if (!library.data.environments?.profiles?.some(item => item.id === value && item.available))
                return false;
            AppLaunch.run(["kitty", "--class", "nacre-control", "--title", row.name, "--", NacrePaths.helperBinary, "software-environments", "run", row.environmentOperation, value]);
            return true;
        }
        if (operation === "font") {
            DesktopSettings.set("interfaceFont", value);
            return true;
        }
        if (operation === "setting-toggle") {
            DesktopSettings.set(value, DesktopSettings.data[value] === false);
            return true;
        }
        if (operation === "toggle") {
            if (value === "dnd")
                DesktopSettings.set("dnd", !DesktopSettings.data.dnd);
            else if (value === "night-light")
                NacreControlTools.toggleNightLight();
            else if (value === "stay-awake")
                work("tool", value);
            else if (value === "mute")
                NacreAudio.toggleMute();
            else if (value === "mic-mute")
                NacreAudio.toggleMic();
            else if (value === "bluetooth" && NacreBluetooth.available)
                NacreBluetooth.adapter.enabled = !NacreBluetooth.powered;
            else if (value === "wifi" && NacreNetwork.wifiAvailable !== false)
                AppLaunch.run(["nmcli", "radio", "wifi", NacreNetwork.wifiEnabled ? "off" : "on"]);
            return true;
        }
        if (operation === "copy") {
            AppLaunch.run(["wl-copy", "--", value]);
            return true;
        }
        if (operation === "copy-clip") {
            DesktopExtras.request("copy", {
                id: value
            });
            return true;
        }
        if (operation === "note") {
            DesktopExtras.request("explore", {
                path: value
            });
            return true;
        }
        if (operation === "focus")
            return NacreCompositor.focusWindow(value);
        if (operation === "url" && /^https?:\/\//.test(value)) {
            AppLaunch.run(["xdg-open", value]);
            return true;
        }
        if (operation === "command" && Array.isArray(row.arguments) && row.arguments.length) {
            AppLaunch.run(row.arguments, row.directory);
            return true;
        }
        if (operation === "tool") {
            if (value === "install-mise") {
                if (!snapshot.capabilities.packages)
                    return false;
                terminal("install", "mise", NacreI18n.tr("launcher.catalog.software.development.mise.label", "Install mise"));
                return true;
            }
            if (value.startsWith("capture-") || ["record-start", "record-screen", "color-picker"].includes(value)) {
                captureDelay.value = value;
                captureDelay.restart();
            } else if (["updates", "logs"].includes(value))
                terminal("tool", value, value === "updates" ? "System updates" : "Nacre logs");
            else
                work("tool", value);
            return true;
        }
        if (["install-package", "remove-package"].includes(operation)) {
            if (!snapshot.capabilities.packages)
                return false;
            terminal(operation === "install-package" ? "install" : "remove", value, (operation === "install-package" ? "Install " : "Remove ") + row.name);
            return true;
        }
        if (operation === "software-open") {
            const entry = softwareRows("software.discover").find(item => item.package === value);
            const app = NacreApps.list.find(item => item.id === entry?.desktopId);
            return app ? NacreApps.launch(app) : false;
        }
        if (operation === "software-theme") {
            NacreWallpapers.preference({
                palettePreset: value,
                paletteMode: row.themeMode || NacreWallpapers.appearancePreferences.paletteMode || "dark"
            });
            return true;
        }
        if (operation === "software-theme-remove")
            return library.work("theme-remove", value);
        if (operation.startsWith("software-extension-")) {
            const item = (library.data.extensions || []).find(item => item.id === value);
            if (!item)
                return false;
            const action = operation.slice("software-extension-".length);
            if (action === "toggle")
                return library.work(item.enabled ? "disable" : "enable", value);
            if (action === "action")
                return library.work("open", value, {
                    action: row.extensionAction
                });
            if (action === "folder") {
                AppLaunch.run(["xdg-open", item.folder]);
                return true;
            }
            if (action === "theme")
                return library.work("extension-theme", value);
            if (action === "install")
                return library.work("install-builtin", value);
            return library.work(action, value);
        }
        if (operation === "webapp") {
            AppLaunch.run(["nacre-menu", "webapp-launch", value]);
            return true;
        }
        if (operation === "webapp-remove") {
            work("webapp-remove", value);
            return true;
        }
        if (operation === "timer-cancel") {
            work("cancel-reminder", value);
            return true;
        }
        if (operation === "config") {
            terminal("config", value, "Nacre configuration");
            return true;
        }
        if (operation === "maintenance" && ["check", "restart"].includes(value)) {
            AppLaunch.run(["kitty", "--class", "nacre-control", "--title", "Nacre maintenance", "--", "python3", tools + "/maintenance.py", value]);
            return true;
        }
        if (operation === "ask" && canAsk && snapshot.capabilities.ai) {
            const view = NacrePanelState.getForActive();
            NacreAiViews.edge.command({
                action: "new"
            });
            NacreAiViews.edge.command({
                action: "send",
                text: value,
                attachments: []
            });
            if (view) {
                view.left = true;
                view.leftPinned = false;
            }
            return true;
        }
        if (operation === "power") {
            if (value === "logout")
                return NacreCompositor.logout();
            const commands = {
                lock: ["loginctl", "lock-session"],
                suspend: ["systemctl", "suspend"],
                reboot: ["systemctl", "reboot"],
                poweroff: ["systemctl", "poweroff"]
            };
            if (commands[value]) {
                AppLaunch.run(commands[value]);
                return true;
            }
        }
        return false;
    }
    function terminal(action, value, title) {
        AppLaunch.run(["kitty", "--class", "nacre-control", "--title", title, "--", NacrePaths.helperBinary, "menu-backend", action, value]);
    }
    function work(action, value = "", payload = {}) {
        if (shared)
            return shared.work(action, value, payload);
        if (actionWorker.running) {
            message = NacreI18n.tr("ui.menu-context.message.another-menu-action-is-finishing", "Another menu action is finishing");
            return false;
        }
        actionWorker.action = action;
        actionWorker.value = value;
        actionWorker.payload = payload;
        actionWorker.running = true;
        return true;
    }
    function inspectApp(entry) {
        previews.inspect(entry?.file || entry?.note ? entry.filePath || entry.value : "");
        if (entry?.kind !== "app")
            return;
        ownershipWanted = entry.id;
        if (!ownershipWorker.running) {
            ownershipWorker.ident = entry.id;
            ownershipWorker.running = true;
        }
    }
    function supportPackage(entry) {
        return ({
                dictation: "voxtype-bin",
                annotate: "satty",
                convert: "ffmpeg",
                share: "localsend",
                record: "wf-recorder",
                ocr: "tesseract",
                qr: "zbar",
                "color-picker": "hyprpicker",
                capture: "grim"
            })[entry?.capability] || (entry?.value === "night-light" ? "hyprsunset" : "");
    }
    NacreHelperTask {
        id: fileWorker
        property string query: ""
        command: [NacrePaths.helperBinary, "menu-backend", "files", query]
        onOutput: text => {
            if (fileWorker.query !== root.fileWanted)
                return;
            try {
                const data = JSON.parse(text);
                root.fileQuery = fileWorker.query;
                root.fileRows = data.rows || [];
                root.fileMessage = data.error || data.message || "";
            } catch (failure) {
                root.message = NacreI18n.tr("ui.menu-context.message.could-not-read-files-result", "Could not read files result");
            }
        }
        onExited: if (query !== root.fileWanted)
            fileDelay.restart()
    }
    Timer {
        id: fileDelay
        interval: 160
        onTriggered: {
            if (fileWorker.running)
                return;
            fileWorker.query = root.fileWanted;
            if (!fileWorker.query.trim())
                return;
            fileWorker.running = true;
        }
    }
    NacreHelperTask {
        id: conversionWorker
        property string query: ""
        command: [NacrePaths.helperBinary, "menu-backend", "convert", query]
        onOutput: text => {
            if (conversionWorker.query !== root.conversionWanted)
                return;
            try {
                const data = JSON.parse(text);
                root.conversionQuery = conversionWorker.query;
                root.conversion = data;
            } catch (failure) {
                root.message = NacreI18n.tr("ui.menu-context.message.could-not-read-convert-result", "Could not read convert result");
            }
        }
        onExited: if (query !== root.conversionWanted)
            conversionDelay.restart()
    }
    Timer {
        id: conversionDelay
        interval: 160
        onTriggered: {
            if (conversionWorker.running)
                return;
            conversionWorker.query = root.conversionWanted;
            if (!conversionWorker.query.trim())
                return;
            conversionWorker.running = true;
        }
    }
    NacreHelperTask {
        id: ownershipWorker
        property string ident: ""
        command: [NacrePaths.helperBinary, "menu-backend", "ownership", ident]
        onOutput: text => {
            try {
                const data = JSON.parse(text);
                if (typeof data.package === "string")
                    root.ownership = Object.assign({}, root.ownership, {
                        [ownershipWorker.ident]: data.package
                    });
            } catch (failure) {
                console.warn("NacreMenuContext: Cannot decode application package ownership:", failure);
            }
        }
        onExited: if (root.ownershipWanted !== ident) {
            ident = root.ownershipWanted;
            running = true;
        }
    }
    FileView {
        path: root.shared ? "" : root.tools + "/launcher-menu.json"
        preload: true
        printErrors: false
        watchChanges: true
        onFileChanged: reload()
        onLoaded: {
            root.defaultText = text();
            root.parse();
        }
    }
    FileView {
        path: root.shared ? "" : NacrePaths.config + "/launcher-menu.json"
        preload: true
        printErrors: false
        watchChanges: true
        onFileChanged: reload()
        onLoaded: {
            root.extensionText = text();
            root.parse();
            root.refresh(true);
        }
    }
    NacreHelperTask {
        id: stateWorker
        property bool force: false
        command: [NacrePaths.helperBinary, "menu-backend", "state", ...(force ? ["--force"] : [])]
        onOutput: text => {
            try {
                const data = JSON.parse(text);
                if (data.error)
                    root.message = data.error;
                else if (data.version === 1 && data.capabilities && Array.isArray(data.packages))
                    root.snapshot = data;
            } catch (failure) {
                root.message = NacreI18n.tr("ui.menu-context.message.menu-status-could-not-be-refreshed", "Menu status could not be refreshed");
            }
        }
    }
    Process {
        id: actionWorker
        property string action: ""
        property string value: ""
        property var payload: ({})
        stdinEnabled: true
        command: [NacrePaths.helperBinary, "menu-backend", action, value]
        onStarted: write(JSON.stringify(payload) + "\n")
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    const data = JSON.parse(text);
                    root.message = data.error || data.message || "";
                } catch (failure) {
                    root.message = NacreI18n.tr("ui.menu-context.message.the-menu-action-could-not-be-completed", "The menu action could not be completed");
                }
            }
        }
        onExited: root.refresh(true)
    }
    NacreHelperTask {
        id: packageWorker
        property string query: ""
        command: [NacrePaths.helperBinary, "menu-backend", "packages", query]
        onOutput: text => {
            if (packageWorker.query !== root.packageWanted)
                return;
            try {
                const data = JSON.parse(text);
                root.packageRows = data.rows || [];
                root.packageQuery = packageWorker.query;
                root.packageMessage = data.error || data.message || "";
            } catch (failure) {
                root.packageMessage = "Package search could not complete";
            }
        }
        onExited: if (query !== root.packageWanted)
            packageDelay.restart()
    }
    Timer {
        id: packageDelay
        interval: 220
        onTriggered: {
            if (packageWorker.running)
                return;
            packageWorker.query = root.packageWanted;
            packageWorker.running = true;
        }
    }
    Timer {
        id: captureDelay
        property string value: ""
        interval: 320
        onTriggered: if (value === "capture-qr")
            root.workflows.work("qr")
        else if (["capture-area", "capture-screen", "capture-window"].includes(value))
            root.workflows.work("capture", value)
        else
            AppLaunch.run([NacrePaths.helperBinary, "menu-backend", "tool", value])
    }
    FileView {
        path: root.shared ? "" : root.tools + "/emoji-catalog.json"
        preload: true
        printErrors: false
        onLoaded: {
            try {
                root.localEmojiCatalog = JSON.parse(text()).entries || [];
            } catch (failure) {
                root.localEmojiCatalog = [];
            }
        }
    }
    Component.onCompleted: if (!shared)
        refresh()
    Process {
        id: brainWorker
        property string query: ""
        stdinEnabled: true
        command: ["python3", root.tools + "/desktop-extras.py", "brain"]
        onStarted: {
            write(JSON.stringify({
                query: query
            }));
            stdinEnabled = false;
        }
        stdout: StdioCollector {
            onStreamFinished: {
                if (brainWorker.query !== root.brainWanted)
                    return;
                try {
                    const data = JSON.parse(text);
                    root.brainQuery = brainWorker.query;
                    root.notes = data.result || [];
                } catch (failure) {
                    root.message = NacreI18n.tr("ui.menu-context.message.could-not-read-brain-results", "Could not read Brain results");
                }
            }
        }
        onExited: if (query !== root.brainWanted)
            brainDelay.restart()
    }
    Timer {
        id: brainDelay
        interval: 180
        onTriggered: if (!brainWorker.running) {
            brainWorker.query = root.brainWanted;
            brainWorker.stdinEnabled = true;
            brainWorker.running = true;
        }
    }
}
