import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Services.UPower
import "../modules/launcher/menu-model.js" as Model
import "../modules/launcher/app-browser.js" as Legacy

Scope {
    id: root
    property var shared: null
    property string brainWanted: ""
    property string brainQuery: ""
    property var notes: []
    readonly property string tools: Quickshell.env("HOME") + "/.local/share/nacre/shell/tools"
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
    readonly property var rows: catalog.rows
    readonly property var navigation: Model.rail(rows)
    readonly property var apps: NacreApps.list.map(entry => Model.application(entry))
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
            catalog = Model.merge(defaultText, extensionText);
        } catch (failure) {
            message = "The menu catalog could not be loaded";
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
    function prepare(route, query = "", opened = false) {
        if (opened) {
            refresh();
            DesktopExtras.request("keys", {});
        }
        const node = rows.find(item => item.id === route);
        if (node?.provider === "clipboard")
            DesktopExtras.request("clips", {});
        if (node?.provider === "keys")
            DesktopExtras.request("keys", {});
        if (route.startsWith("health"))
            Maintenance.refresh();
        if (opened)
            NacreControlTools.refresh();
        const scope = Model.scope(query);
        if (scope.kind === "files") {
            fileWanted = scope.text;
            fileDelay.restart();
        }
        if (scope.kind === "calculator" && /\b(to|in)\b/.test(scope.text)) {
            conversionWanted = scope.text;
            conversionDelay.restart();
        }
        if (scope.kind === "brain") {
            brainWanted = scope.text;
            brainDelay.restart();
        }
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
        result.status = row.state === "updates" ? (Updates.count ? Updates.count + " available" : "Up to date") : typeof checked === "boolean" ? (["setting", "dnd", "night-light", "stay-awake", "mute", "mic-mute", "wifi", "bluetooth"].includes(row.state) ? (checked ? "On" : "Off") : checked ? "In use" : "") : "";
        result.disabled = !!row.capability && snapshot.capabilities[row.capability] !== true;
        result.reason = result.disabled ? "Optional tool is not installed" : "";
        if (row.operation === "media") {
            const player = NacrePlayers.active;
            const capability = {
                playPause: "canTogglePlaying",
                next: "canGoNext",
                previous: "canGoPrevious"
            }[row.value];
            result.disabled = !player || !player[capability];
            result.reason = !player ? "No active media player" : result.disabled ? "This player does not support that action" : "";
        }
        if (row.value === "record-stop") {
            result.disabled = !snapshot.recording;
            result.reason = result.disabled ? "No recording is running" : "";
        }
        if (row.value === "record-start" && snapshot.recording) {
            result.disabled = true;
            result.reason = "A recording is already running";
        }
        if (row.value === "night-light") {
            result.disabled = !NacreControlTools.data.nightLightSupported || !!NacreControlTools.data.nightLightExternal;
            result.reason = NacreControlTools.data.nightLightExternal ? "Managed by an existing controller" : result.disabled ? "Install hyprsunset to use Night light" : "";
        }
        if (row.operation === "power-profile" && row.value === "performance" && !PowerProfiles.hasPerformanceProfile) {
            result.disabled = true;
            result.reason = "Performance profile is unavailable on this device";
        }
        if (row.value === "mute")
            result.disabled = !NacreAudio.available;
        if (row.value === "mic-mute")
            result.disabled = !NacreAudio.micAvailable;
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
    function provider(route, query = "") {
        if (root.node(route)?.provider === "audio")
            return (NacreAudio.outputs || []).map(item => valueRow(route, "output-" + item.id, item.description || item.nickname || item.name, "audio-output", String(item.id), "speaker", "Audio output", {
                    checked: NacreAudio.sink === item,
                    disabled: !item.ready
                }));
        const node = rows.find(row => row.id === route);
        if (!node?.provider)
            return [];
        if (node.provider === "apps")
            return apps;
        if (node.provider === "wallpapers")
            return NacreWallpapers.list.map(entry => valueRow(route, Qt.md5(entry.path), entry.name, "wallpaper", entry.path, "wallpaper", entry.dynamic ? "Animated wallpaper" : "Wallpaper", {
                    preview: entry.thumbnail || entry.poster,
                    checked: entry.path === NacreWallpapers.current,
                    status: entry.path === NacreWallpapers.current ? "In use" : ""
                }));
        if (node.provider === "accents")
            return NacreWallpapers.paletteOptions.map((entry, index) => valueRow(route, "choice-" + index, entry.name || "Color " + (index + 1), "accent", entry.accent, "palette", "Use this color family", {
                    colors: entry.colors || [],
                    checked: entry.accent === NacreWallpapers.selectedAccent,
                    status: entry.accent === NacreWallpapers.selectedAccent ? "In use" : ""
                }));
        if (node.provider === "fonts")
            return [valueRow(route, "default", "Nacre default", "font", "", "text_fields", "IBM Plex Sans", {
                    checked: !DesktopSettings.data.interfaceFont
                }), ...(snapshot.fonts || []).map(name => valueRow(route, Qt.md5(name), name, "font", name, "text_fields", "Preview this interface font", {
                        previewFont: name,
                        disabled: name.length > 80,
                        checked: DesktopSettings.data.interfaceFont === name
                    }))];
        if (node.provider === "windows")
            return (NacreCompositor.windows || []).map(client => valueRow(route, String(client.windowId), client.title || client.wmClass, "focus", client.windowId, "window", "Workspace " + client.workspace?.name));
        if (node.provider === "wifi")
            return NacreNetwork.visibleNetworks.map(point => valueRow(route, point.bssid.replace(/:/g, ""), point.ssid || "Hidden network", "settings", "network", "wifi", point.strength + "% signal", {
                    checked: point.active,
                    status: point.active ? "Connected" : ""
                }));
        if (node.provider === "bluetooth")
            return NacreBluetooth.devices.map(device => valueRow(route, device.address.replace(/:/g, ""), device.alias, "settings", "bluetooth", "bluetooth", device.paired ? "Paired device" : "Nearby device", {
                    checked: device.connected,
                    status: device.connected ? "Connected" : ""
                }));
        if (node.provider === "clipboard")
            return DesktopExtras.clips.map(entry => valueRow(route, String(entry.id), entry.label || "Clipboard item", "copy-clip", entry.id, "content_paste", entry.kind || "Copied item", {
                    preview: entry.preview || ""
                }));
        if (node.provider === "keys")
            return DesktopExtras.keys.map((entry, index) => valueRow(route, "binding-" + index, entry.description || entry.key, "info", "shortcut", "keyboard", entry.key, {
                    shortcut: entry.key,
                    disabled: false
                }));
        if (node.provider === "emoji")
            return emojiData.map((entry, index) => valueRow(route, Qt.md5(entry.value), entry.name, "copy", entry.value, "mood", "Copy this emoji", {
                    emoji: entry.value
                }));
        if (node.provider === "timers")
            return (snapshot.reminders || []).map(entry => valueRow(route, entry.unit.slice(0, -6), entry.label, "timer-cancel", entry.unit, "timer", "Cancel this reminder", {
                    status: "Until " + Qt.formatTime(new Date(entry.due * 1000), "hh:mm")
                }));
        if (node.provider === "webapps")
            return (snapshot.webapps || []).map(entry => valueRow(route, entry.id, entry.label, "webapp", entry.id, "public", entry.url, {
                    webapp: true
                }));
        if (node.provider === "installed")
            return (snapshot.packages || []).filter(entry => !query || Model.normalized(entry.name).includes(Model.normalized(query))).slice(0, 100).map(entry => valueRow(route, Qt.md5(entry.name), entry.name, "remove-package", entry.name, "inventory_2", entry.version, {
                    package: entry.name,
                    status: "Installed"
                }));
        if (node.provider === "software")
            return catalog.software.map(entry => valueRow(route, Qt.md5(entry.package), entry.label, "install-package", entry.package, "download", entry.description, {
                    package: entry.package,
                    checked: packageNames.has(entry.package),
                    status: packageNames.has(entry.package) ? "Installed" : "Install",
                    disabled: packageNames.has(entry.package) || !snapshot.capabilities.packages,
                    reason: packageNames.has(entry.package) ? "Installed; use item actions to remove it" : !snapshot.capabilities.packages ? "Requires an Arch-compatible package manager" : ""
                }));
        if (node.provider === "packages")
            return packageQuery === Model.scope(query).text ? packageRows.map(entry => valueRow(route, Qt.md5(entry.name), entry.name, "install-package", entry.name, "download", entry.description, {
                    package: entry.name,
                    status: packageNames.has(entry.name) ? "Installed" : entry.repository,
                    checked: packageNames.has(entry.name),
                    disabled: packageNames.has(entry.name)
                })) : [];
        return [];
    }
    readonly property var emojiData: [
        {
            value: "😀",
            name: "Grinning face"
        },
        {
            value: "😂",
            name: "Laughing"
        },
        {
            value: "🥹",
            name: "Holding back tears"
        },
        {
            value: "😊",
            name: "Smiling"
        },
        {
            value: "😍",
            name: "Heart eyes"
        },
        {
            value: "🤔",
            name: "Thinking"
        },
        {
            value: "😎",
            name: "Cool"
        },
        {
            value: "😭",
            name: "Crying"
        },
        {
            value: "😅",
            name: "Nervous laugh"
        },
        {
            value: "😴",
            name: "Sleeping"
        },
        {
            value: "👍",
            name: "Thumbs up"
        },
        {
            value: "👎",
            name: "Thumbs down"
        },
        {
            value: "👏",
            name: "Clapping"
        },
        {
            value: "🙏",
            name: "Thank you"
        },
        {
            value: "💪",
            name: "Strong"
        },
        {
            value: "👋",
            name: "Wave"
        },
        {
            value: "❤️",
            name: "Heart"
        },
        {
            value: "💜",
            name: "Purple heart"
        },
        {
            value: "💙",
            name: "Blue heart"
        },
        {
            value: "💚",
            name: "Green heart"
        },
        {
            value: "🔥",
            name: "Fire"
        },
        {
            value: "✨",
            name: "Sparkles"
        },
        {
            value: "🎉",
            name: "Celebrate"
        },
        {
            value: "✅",
            name: "Done"
        },
        {
            value: "❌",
            name: "No"
        },
        {
            value: "⚠️",
            name: "Warning"
        },
        {
            value: "🚀",
            name: "Rocket"
        },
        {
            value: "🐚",
            name: "Shell"
        },
        {
            value: "🫧",
            name: "Bubbles"
        },
        {
            value: "🌊",
            name: "Wave ocean"
        },
        {
            value: "☕",
            name: "Coffee"
        },
        {
            value: "💻",
            name: "Laptop"
        },
        {
            value: "🧠",
            name: "Brain"
        },
        {
            value: "🌙",
            name: "Moon"
        },
        {
            value: "🌈",
            name: "Rainbow"
        },
        {
            value: "💡",
            name: "Idea"
        },
        {
            value: "🛠️",
            name: "Tools"
        },
        {
            value: "📌",
            name: "Pinned"
        },
        {
            value: "📷",
            name: "Camera"
        },
        {
            value: "🎵",
            name: "Music"
        }
    ]
    function favorites() {
        const needed = rows.filter(row => row.provider && LauncherPreferences.favorites.some(id => id.startsWith("nacre-menu:" + row.id + ".")));
        const candidates = allStatic().concat(...needed.map(row => provider(row.id)));
        return Model.alphabetical(LauncherPreferences.favorites.map(id => Model.favorite(id, apps, candidates, legacy)).filter(Boolean));
    }
    function recent() {
        const candidates = allStatic();
        return LauncherPreferences.recent.map(id => Model.favorite(id, apps, candidates, legacy)).filter(row => row && !keepOpen(row)).slice(0, 5);
    }
    function brainRows(query) {
        if (brainQuery !== query)
            return [];
        return notes.map(entry => valueRow("actions.assistant.search", Qt.md5(entry.path), entry.title, "note", entry.path, "description", entry.preview, {
                breadcrumb: "Nacre Brain",
                note: true
            }));
    }
    function globalRows() {
        return apps.concat(allStatic(), provider("style.wallpapers"), provider("software.discover"));
    }
    function ranked(items, query) {
        return Model.ranked(items, query, LauncherPreferences.usage || {}, LauncherPreferences.recent);
    }
    function content(route, query) {
        const scope = Model.scope(query);
        if (scope.kind === "files")
            return fileQuery === scope.text ? fileRows.map(item => valueRow("actions.files", item.id, item.name, "open-file", item.path, "description", item.path)) : [];
        if (scope.kind === "calculator")
            return [];
        if (scope.kind === "brain")
            return brainRows(scope.text);
        if (scope.kind === "ai")
            return scope.text ? [valueRow("actions.assistant", "question", "Ask Nacre AI", "ask", scope.text, "chat", scope.text, {
                    brand: "ai",
                    disabled: !canAsk || !snapshot.capabilities.ai,
                    reason: !snapshot.capabilities.ai ? "Nacre AI is optional; open Setup to add it" : !canAsk ? "An assistant task or unsent draft is active; open Nacre AI instead" : ""
                })] : [];
        if (scope.kind === "help")
            return [
                {
                    id: "scope-actions",
                    kind: "action",
                    name: "> Actions",
                    label: "> Actions",
                    description: "Search desktop commands",
                    operation: "scope",
                    value: "> ",
                    icon: "bolt"
                },
                {
                    id: "scope-calculator",
                    kind: "action",
                    name: "= Calculator",
                    label: "= Calculator",
                    description: "Calculate and copy a result",
                    operation: "scope",
                    value: "= ",
                    icon: "calculate"
                },
                {
                    id: "scope-brain",
                    kind: "action",
                    name: "# Brain",
                    label: "# Brain",
                    description: "Search private notes",
                    operation: "scope",
                    value: "# ",
                    icon: "neurology"
                },
                {
                    id: "scope-files",
                    kind: "action",
                    name: ": Files",
                    label: ": Files",
                    description: "Search your usual folders",
                    operation: "scope",
                    value: ": ",
                    icon: "folder"
                },
                {
                    id: "scope-ai",
                    kind: "action",
                    name: "? Nacre AI",
                    label: "? Nacre AI",
                    description: "Ask an explicit question",
                    operation: "scope",
                    value: "? ",
                    icon: "chat"
                }
            ];
        if (scope.kind === "actions")
            return ranked(allStatic().filter(row => row.kind !== "group"), scope.text);
        if (route === "software.packages")
            return provider(route, scope.text);
        if (scope.text) {
            if (!["home", "apps", "hidden"].includes(route)) {
                const local = allStatic().filter(row => row.id.startsWith(route + "."));
                const groups = rows.filter(row => row.provider && (row.id === route || row.id.startsWith(route + ".")));
                return ranked(local.concat(...groups.map(row => provider(row.id, scope.text))), scope.text);
            }
            return ranked(globalRows(), scope.text).concat([valueRow("actions", "search-web", "Search the web for “" + scope.text.slice(0, 100) + "”", "search-web", scope.text, "public", "Opens your browser only when selected")]);
        }
        if (route === "home")
            return favorites();
        if (route === "apps")
            return apps;
        if (route === "hidden")
            return NacreApps.all.filter(entry => LauncherPreferences.hidden.includes(entry.id)).map(entry => Object.assign(Model.application(entry), {
                    hidden: true
                }));
        return Model.children(route, rows).map(decorate).concat(provider(route));
    }
    function record(row) {
        if (!keepOpen(row) && row.operation !== "ask" && row.kind !== "calculation" && row.operation !== "scope" && row.operation !== "info")
            LauncherPreferences.update("record", Model.stable(row), true);
    }
    function keepOpen(row) {
        return ["toggle", "setting-toggle", "personality", "background", "palette-mode", "accent", "wallpaper", "next-wallpaper", "font", "info", "scope", "form", "timer-cancel", "webapp-remove", "power-profile", "media", "audio-output"].includes(row.operation);
    }
    function execute(row, confirmed = false) {
        if (shared)
            return shared.execute(row, confirmed);
        if (!row || row.disabled || row.kind === "group")
            return false;
        if ((row.confirm || row.operation === "power" && row.value !== "lock" || row.operation === "maintenance" && row.value === "restart") && !confirmed)
            return false;
        if (row.operation === "media")
            return NacrePlayers.control(row.value);
        if (row.operation === "audio-output")
            return NacreAudio.chooseOutput(row.value);
        if (row.operation === "open-file") {
            AppLaunch.run(["xdg-open", row.value]);
            return true;
        }
        if (row.operation === "search-web") {
            AppLaunch.run(["nacre-app", "browser", "https://www.google.com/search?q=" + encodeURIComponent(row.value)]);
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
            else if (value === "wifi")
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
            AppLaunch.run(row.arguments);
            return true;
        }
        if (operation === "tool") {
            if (value.startsWith("capture-") || ["record-start", "color-picker"].includes(value)) {
                captureDelay.value = value;
                captureDelay.restart();
            } else if (["updates", "logs"].includes(value))
                terminal("tool", value, value === "updates" ? "System updates" : "Nacre logs");
            else
                work("tool", value);
            return true;
        }
        if (["install-package", "remove-package"].includes(operation)) {
            terminal(operation === "install-package" ? "install" : "remove", value, (operation === "install-package" ? "Install " : "Remove ") + row.name);
            return true;
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
        AppLaunch.run(["kitty", "--class", "nacre-control", "--title", title, "--", "python3", tools + "/menu-backend.py", action, value]);
    }
    function work(action, value = "", payload = {}) {
        if (shared)
            return shared.work(action, value, payload);
        if (actionWorker.running) {
            message = "Another menu action is finishing";
            return false;
        }
        actionWorker.action = action;
        actionWorker.value = value;
        actionWorker.payload = payload;
        actionWorker.running = true;
        return true;
    }
    function inspectApp(entry) {
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
                record: "wf-recorder",
                ocr: "tesseract",
                qr: "zbar",
                "color-picker": "hyprpicker",
                capture: "grim"
            })[entry?.capability] || (entry?.value === "night-light" ? "hyprsunset" : "");
    }
    Process {
        id: fileWorker
        property string query: ""
        command: ["python3", root.tools + "/menu-backend.py", "files", query]
        stdout: StdioCollector {
            onStreamFinished: {
                if (fileWorker.query !== root.fileWanted)
                    return;
                try {
                    const data = JSON.parse(text);
                    root.fileQuery = fileWorker.query;
                    root.fileRows = data.rows || [];
                    root.fileMessage = data.error || data.message || "";
                } catch (failure) {
                    root.message = "Could not read files result";
                }
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
    Process {
        id: conversionWorker
        property string query: ""
        command: ["python3", root.tools + "/menu-backend.py", "convert", query]
        stdout: StdioCollector {
            onStreamFinished: {
                if (conversionWorker.query !== root.conversionWanted)
                    return;
                try {
                    const data = JSON.parse(text);
                    root.conversionQuery = conversionWorker.query;
                    root.conversion = data;
                } catch (failure) {
                    root.message = "Could not read convert result";
                }
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
    Process {
        id: ownershipWorker
        property string ident: ""
        command: ["python3", root.tools + "/menu-backend.py", "ownership", ident]
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    const data = JSON.parse(text);
                    if (typeof data.package === "string")
                        root.ownership = Object.assign({}, root.ownership, {
                            [ownershipWorker.ident]: data.package
                        });
                } catch (failure) {}
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
        path: root.shared ? "" : Quickshell.env("HOME") + "/.config/nacre/launcher-menu.json"
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
    Process {
        id: stateWorker
        property bool force: false
        command: ["python3", root.tools + "/menu-backend.py", "state", ...(force ? ["--force"] : [])]
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    const data = JSON.parse(text);
                    if (data.error)
                        root.message = data.error;
                    else if (data.version === 1 && data.capabilities && Array.isArray(data.packages))
                        root.snapshot = data;
                } catch (failure) {
                    root.message = "Menu status could not be refreshed";
                }
            }
        }
    }
    Process {
        id: actionWorker
        property string action: ""
        property string value: ""
        property var payload: ({})
        stdinEnabled: true
        command: ["python3", root.tools + "/menu-backend.py", action, value]
        onStarted: write(JSON.stringify(payload) + "\n")
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    const data = JSON.parse(text);
                    root.message = data.error || data.message || "";
                } catch (failure) {
                    root.message = "The menu action could not be completed";
                }
            }
        }
        onExited: root.refresh(true)
    }
    Process {
        id: packageWorker
        property string query: ""
        command: ["python3", root.tools + "/menu-backend.py", "packages", query]
        stdout: StdioCollector {
            onStreamFinished: {
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
        onTriggered: AppLaunch.run(["python3", root.tools + "/menu-backend.py", "tool", value])
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
                    root.message = "Could not read Brain results";
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
