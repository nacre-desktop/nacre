pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io
import qs.utils
import qs.modules.settings.prototype

SettingsStore {
    id: root
    property var inputQueue: []
    property var inputDesired: ({})
    property int inputRevision: 0
    property string inputError: ""
    property bool colorMigrationRequested: false
    function refreshColors() {
        const preferences = NacreSettingsStorage.documents["wallpaper-picker"];
        if (!colorMigrationRequested && preferences && !NacreSettingsStorage.errors["wallpaper-picker"] && preferences.paletteAutoMode === undefined && NacreSettingsStorage.documents.colors?.ui?.modeChoice === "wallpaper") {
            colorMigrationRequested = true;
            NacreWallpapers.preference({
                paletteAutoMode: true
            });
        }
    }
    function issue(page) {
        return error || (["colors", "wallpaper"].includes(page) ? NacreWallpapers.error : "") || "";
    }
    function inputChange(setting, value) {
        const key = setting.store.key[0], revision = ++inputRevision;
        inputDesired = Object.assign({}, inputDesired, {
            [key]: {
                value,
                revision
            }
        });
        inputQueue = [...inputQueue.filter(item => item.key !== key),
            {
                key,
                value,
                revision
            }
        ];
        nextInput();
    }
    function nextInput() {
        if (inputWorker.running || !inputQueue.length || DesktopSettings.busy || (DesktopSettings.queue || []).length)
            return;
        inputWorker.change = inputQueue[0];
        inputQueue = inputQueue.slice(1);
        const environment = NacrePaths.niriDesktopConfig ? ["env", "NACRE_NIRI_DESKTOP_CONFIG=" + NacrePaths.niriDesktopConfig] : [];
        inputWorker.command = [...environment, NacrePaths.helperBinary, "input-preferences", inputWorker.change.key === "keyboardLayouts" ? "layouts" : "touchpad", String(inputWorker.change.value)];
        inputError = "";
        inputWorker.running = true;
    }
    readonly property Process inputWorker: Process {
        property var change: null
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    const result = JSON.parse(text);
                    root.inputError = result.ok ? "" : result.error;
                } catch (failure) {
                    root.inputError = NacreI18n.tr("settings.registry.input.failed", "Input preferences could not be applied.");
                }
            }
        }
        onExited: code => {
            if (code !== 0 && root.inputDesired[change.key]?.revision === change.revision) {
                const values = Object.assign({}, root.inputDesired);
                delete values[change.key];
                root.inputDesired = values;
                if (!root.inputError)
                    root.inputError = NacreI18n.tr("settings.registry.input.failed", "Input preferences could not be applied.");
            }
            NacreSettingsStorage.request("software-input");
            root.nextInput();
        }
    }
    readonly property Binding inputWriteGuard: Binding {
        target: DesktopSettings
        property: "externalWriteBlocked"
        value: root.inputWorker.running
    }
    readonly property Connections desktopWriteCompletion: Connections {
        target: DesktopSettings
        function onBusyChanged() {
            root.nextInput();
        }
        function onQueueChanged() {
            root.nextInput();
        }
    }
    readonly property Connections inputAcknowledgements: Connections {
        target: NacreSettingsStorage
        function onDocumentsChanged() {
            if (NacrePanelState.settingsPage === "colors")
                root.refreshColors();
            const values = Object.assign({}, root.inputDesired), saved = NacreSettingsStorage.documents["software-input"] || {};
            for (const key of Object.keys(values)) {
                if (saved[key] === values[key].value || key === "keyboardLayouts" && values[key].value === "" && saved[key] === undefined)
                    delete values[key];
            }
            root.inputDesired = values;
        }
    }
    property var registryObject: NacreSettingsRegistry
    readonly property FileView keyboardFile: FileView {
        path: "/usr/share/X11/xkb/rules/base.lst"
        blockLoading: true
        printErrors: false
    }
    readonly property var keyboardLayouts: {
        let layout = false;
        const values = [
            {
                value: "",
                label: NacreI18n.tr("settings.registry.ui.default", "System default")
            }
        ];
        for (const line of keyboardFile.text().split("\n")) {
            if (line.startsWith("!")) {
                layout = line.trim() === "! layout";
                continue;
            }
            const row = layout && line.match(/^\s*([a-z][a-z0-9_]*)\s+(.+)$/);
            if (row)
                values.push({
                    value: row[1],
                    label: row[2]
                });
        }
        return values;
    }
    function choiceOptions(setting) {
        return setting.picker === "keyboard" ? keyboardLayouts : [];
    }
    readonly property string collectionFolder: NacreSettingsStorage.documents.cli?.wallpaper?.directory || ""
    onCollectionFolderChanged: NacreWallpapers.refresh()
    schema: registryObject?.schema || ({
            settings: [],
            pages: [],
            sections: []
        })
    capabilities: NacreSettingsCapabilities.snapshot
    files: Object.assign({}, NacreSettingsStorage.documents, {
        "software-input": Object.assign({}, NacreSettingsStorage.documents["software-input"] || {}, (() => {
                const values = {};
                for (const key of Object.keys(inputDesired))
                    values[key] = inputDesired[key].value;
                return values;
            })()),
        "ai-settings": {
            default_assistant: SidebarChat.defaultProvider
        },
        desktop: DesktopSettings.data,
        colors: Object.assign({}, NacreSettingsStorage.documents.colors || {}, {
            ui: Object.assign({}, NacreSettingsStorage.documents.colors?.ui || {}, {
                modeChoice: NacreWallpapers.preferences.paletteAutoMode === true ? "wallpaper" : NacreWallpapers.appearancePreferences.paletteMode || (NacreColours.light ? "light" : "dark")
            })
        }),
        "wallpaper-picker": NacreWallpapers.appearancePreferences,
        dashboard: Object.assign({}, NacreDashboardState.preferences, {
            tabs: NacreDashboardState.coreTabs
        }),
        "dashboard-extras": {
            enabled: NacreDashboardState.enabledExtras,
            order: NacreDashboardState.extraOrder,
            weatherPhotos: NacreDashboardState.weatherPhotos
        },
        "control-center": {
            tiles: NacreControlPreferences.tiles
        },
        launcher: {
            favorites: LauncherPreferences.favorites,
            hidden: LauncherPreferences.hidden,
            recent: LauncherPreferences.recent,
            usage: LauncherPreferences.usage
        }
    })
    availableStores: [...NacreSettingsStorage.registry.filter(item => item.settings?.access === "write").map(item => item.id), "scheme", "ai-settings", "software-input"].filter(id => !NacreSettingsStorage.errors[id])
    profileValues: {
        const presets = {
            focused: ["blur", "shadow", "animations", "dnd"],
            presentation: ["aiSidebarEnabled", "topBarEnabled", "frameTopEnabled", "frameLeftEnabled", "frameRightEnabled", "controlCenterEnabled", "frameBottomEnabled", "dnd"],
            meeting: ["dnd", "blur"],
            music: ["dnd"],
            docked: ["dnd"],
            minimal: ["topBarEnabled", "frameTopEnabled", "clickEdgeMenus", "frameLeftEnabled", "frameRightEnabled", "controlCenterEnabled", "frameBottomEnabled", "frameWidth", "gapsIn", "gapsOut", "borderSize", "shadow"]
        };
        const keys = presets[DesktopSettings.data.preset] || [];
        const values = {};
        for (const setting of schema.settings)
            if (setting.store.id === "desktop" && keys.includes(setting.store.key[0]))
                values[setting.id] = DesktopSettings.data[setting.store.key[0]] ?? setting.default;
        return values;
    }
    error: inputError || Object.values(NacreSettingsStorage.errors).find(value => !!value) || DesktopSettings.error || ""
    function applySetting(setting, value) {
        const id = setting.store.id;
        const key = setting.store.key;
        if (["keyboard-layout", "touchpad-enabled"].includes(setting.adapter))
            inputChange(setting, value);
        else if (id === "ai-settings" && key[0] === "default_assistant")
            SidebarChat.setProvider(value);
        else if (id === "desktop")
            DesktopSettings.set(key[0], value);
        else if (id === "wallpaper-picker")
            NacreWallpapers.preference({
                [key[0]]: value
            });
        else if (id === "dashboard")
            NacreDashboardState.save(key[0], value);
        else if (id === "dashboard-extras" && key[0] === "weatherPhotos") {
            NacreDashboardState.weatherPhotos = value;
            NacreDashboardState.saveExtras();
        } else if (id === "launcher" && ["favorites", "hidden"].includes(key[0])) {
            const old = files.launcher[key[0]] || [];
            for (const item of new Set([...old, ...value]))
                if (old.includes(item) !== value.includes(item))
                    LauncherPreferences.update(key[0] === "favorites" ? "favorite" : "hide", item, value.includes(item));
        } else if (setting.adapter === "color-mode") {
            NacreSettingsStorage.request("colors", key, value);
            NacreColours.setMode(value === "wallpaper" ? "auto" : value);
        } else if (id === "scheme") {
            palette.command = [NacrePaths.helperBinary, "nacre_shell", "--publish", "scheme", "set", key[0] === "flavour" ? "-f" : "-v", value];
            palette.running = true;
        } else
            NacreSettingsStorage.request(id, key, value);
    }
    onWriteRequested: (setting, value) => applySetting(setting, value)
    onResetRequested: settings => {
        for (const setting of settings)
            applySetting(setting, setting.default);
    }
    onActionRequested: action => {
        if (action === "colors-refresh")
            refreshColors();
        else if (action === "power-refresh")
            NacrePowerPolicy.refresh();
        else if (action === "power-fix") {
            if (NacrePowerPolicy.installed)
                AppLaunch.run(["pkexec", "systemctl", "start", "power-profiles-daemon"]);
            else
                NacrePanelState.openMenu("software.packages", "power-profiles-daemon");
        } else if (action === "fonts")
            NacrePanelState.openMenu("style.fonts");
        else if (action === "colors-studio")
            NacreColorsApp.open();
        else if (action === "launcher-history") {
            NacreSettingsStorage.request("launcher", ["recent"], []);
            NacreSettingsStorage.request("launcher", ["usage"], {});
        } else if (action === "notifications-history")
            NacreNotifs.clearHistory();
        else if (action === "clipboard-history")
            AppLaunch.run(["cliphist", "wipe"]);
        else if (action === "lock-preview")
            AppLaunch.run(["nacre-shell", "lock-preview"]);
        else if (action === "lock-now")
            DesktopActions.execute("lock");
        else if (action === "updates")
            NacrePanelState.openControls("updates");
        else if (action === "launcher-entries")
            NacrePanelState.openMenu("actions.saved");
        else if (action === "brain")
            DesktopActions.execute("brain");
        else if (action === "welcome")
            NacreWelcomeApp.open();
        else if (["setup", "install-ai", "install-brain"].includes(action))
            AppLaunch.run(["kitty", "--title", "Nacre setup", "--", "nacre", "setup"]);
        else if (action === "idle-policy")
            AppLaunch.run(["kitty", "--title", "Idle policy", "--", "nacre", "idle"]);
    }
    readonly property NacrePaletteTask palette: NacrePaletteTask {}
}
