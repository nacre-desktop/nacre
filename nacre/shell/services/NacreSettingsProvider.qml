pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io
import qs.utils
import qs.modules.settings.prototype

SettingsStore {
    id: root
    property var registryObject: NacreSettingsRegistry
    schema: registryObject?.schema || ({
            settings: [],
            pages: [],
            sections: []
        })
    capabilities: NacreSettingsCapabilities.snapshot
    files: Object.assign({}, NacreSettingsStorage.documents, {
        "ai-settings": {
            default_assistant: SidebarChat.defaultProvider
        },
        desktop: DesktopSettings.data,
        colors: Object.assign({}, NacreSettingsStorage.documents.colors || {}, {
            ui: Object.assign({}, NacreSettingsStorage.documents.colors?.ui || {}, {
                modeChoice: NacreSettingsStorage.documents.colors?.ui?.modeChoice === "wallpaper" ? "wallpaper" : NacreWallpapers.appearancePreferences.paletteMode || (NacreColours.light ? "light" : "dark")
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
    availableStores: [...NacreSettingsStorage.registry.filter(item => item.settings?.access === "write").map(item => item.id), "scheme", "ai-settings"].filter(id => !NacreSettingsStorage.errors[id])
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
    error: DesktopSettings.message || Object.values(NacreSettingsStorage.errors).find(value => !!value) || ""
    function applySetting(setting, value) {
        const id = setting.store.id;
        const key = setting.store.key;
        if (id === "ai-settings" && key[0] === "default_assistant")
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
            NacreWallpapers.preference(value === "wallpaper" ? {
                paletteAccent: "auto"
            } : {
                paletteMode: value
            });
        } else if (id === "scheme") {
            palette.command = [NacrePaths.palettePython, "-m", "nacre_shell", "scheme", key[0] === "flavour" ? "-f" : "-v", value];
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
        if (action === "power-refresh")
            NacrePowerPolicy.refresh();
        else if (action === "power-fix") {
            if (NacrePowerPolicy.installed)
                AppLaunch.run(["pkexec", "systemctl", "start", "power-profiles-daemon"]);
            else
                NacrePanelState.openMenu("software.packages", "power-profiles-daemon");
        } else if (action === "colors-studio")
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
            AppLaunch.run(["nacre-brain"]);
        else if (action === "welcome")
            NacreWelcomeApp.open();
        else if (["setup", "install-ai", "install-brain"].includes(action))
            AppLaunch.run(["kitty", "--title", "Nacre setup", "--", "nacre", "setup"]);
        else if (action === "idle-policy")
            AppLaunch.run(["kitty", "--title", "Idle policy", "--", "nacre", "idle"]);
    }
    readonly property NacrePaletteTask palette: NacrePaletteTask {}
}
