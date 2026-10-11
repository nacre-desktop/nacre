import QtQuick
import QtTest
import "fixtures"
import "settings"

TestCase {
    id: test

    Component {
        id: sharedSection
        NacreSettingsSection {
            width: 320
            title: "Wrapped settings heading ".repeat(5)
            description: "Detailed description ".repeat(12)
            collapsible: true
            Rectangle {
                objectName: "testSectionContent"
                width: parent.width
                height: 120
            }
        }
    }
    Component {
        id: sharedToggle
        NacreSettingToggle {
            width: 320
            label: "A longer checkbox label that wraps into multiple lines"
            setting: "dnd"
        }
    }
    function test_shared_section_bounds_collapse_and_user_only_toggle() {
        const section = createTemporaryObject(sharedSection, test);
        wait(30);
        const content = findChild(section, "testSectionContent");
        verify(content.mapToItem(section, 0, 0).y + content.height <= section.height - 16 + .01);
        const height = section.height;
        mouseClick(findChild(section, "settingsSectionToggle"), 15, 15);
        verify(!section.expanded);
        verify(section.height < height);
        DesktopSettings.data = {
            dnd: false
        };
        DesktopSettings.writes = [];
        const toggle = createTemporaryObject(sharedToggle, test);
        wait(20);
        compare(DesktopSettings.writes.length, 0);
        verify(!toggle.checked);
        mouseClick(toggle, 14, toggle.height / 2);
        compare(DesktopSettings.writes.length, 1);
        verify(toggle.checked);
        compare(DesktopSettings.writes[0].key, "dnd");
    }
    function test_catalog_plain_labels_external_routes_and_unknown_fallback() {
        const view = createTemporaryObject(settings, test);
        wait(20);
        compare(view.pages.length, 12);
        compare(view.pages[0].label, "Appearance");
        NacrePanelState.settingsPage = "bluetooth";
        wait(20);
        compare(view.page, "bluetooth");
        view.open("unknown");
        compare(view.page, "appearance");
        view.query = "microphone volume";
        compare(view.matches.length, 1);
        compare(view.matches[0].id, "sound");
        view.width = 600;
        wait(30);
        verify(view.narrow);
        const scroll = findChild(view, "settingsScroll");
        verify(scroll.x + scroll.width <= view.width);
        view.query = "no matching setting xyz";
        compare(view.matches.length, 0);
    }

    function test_pages_search_and_visible_only_loading() {
        const view = createTemporaryObject(settings, test);
        verify(view);
        wait(30);
        const loader = findChild(view, "settingsPage");
        verify(loader.item);
        compare(loader.item.page, "desktop");
        view.query = "microphone";
        wait(20);
        verify(!loader.active);
        compare(view.matches.length, 1);
        compare(view.matches[0].id, "sound");
        view.open("sound");
        wait(30);
        verify(loader.item);
        compare(view.page, "sound");
        compare(view.query, "");
        view.active = false;
        wait(20);
        verify(!loader.active);
        verify(!loader.item);
    }

    function test_appearance_interval_writes_only_on_user_input() {
        const prior = NacreWallpapers.preferences;
        NacreWallpapers.preferences = Object.assign({}, prior, {
            rotationMinutes: 30,
            paletteAccent: "aabbcc"
        });
        const view = createTemporaryObject(settings, test);
        view.open("appearance");
        wait(30);
        const field = findChild(findChild(view, "settingsPage").item, "wallpaperInterval");
        compare(field.value, 30);
        field.value = 45;
        compare(NacreWallpapers.preferences.rotationMinutes, 30);
        const scroll = findChild(view, "settingsScroll");
        scroll.contentY = Math.max(0, field.mapToItem(scroll.contentItem, 0, 0).y - 80);
        wait(30);
        mouseClick(field, field.width - 17, field.height / 2);
        compare(NacreWallpapers.preferences.rotationMinutes, 46);
        compare(NacreWallpapers.preferences.paletteAccent, "aabbcc");
        compare(field.from, 5);
        compare(field.to, 1440);
        NacreWallpapers.preferences = prior;
    }

    function test_desktop_display_workflow_and_maintenance_commands_are_explicit() {
        const view = createTemporaryObject(settings, test);
        view.open("desktop");
        wait(30);
        DesktopSettings.writes = [];
        const number = findChild(findChild(view, "settingsPage").item, "desktopNumbergapsIn");
        verify(number);
        number.value = 7;
        compare(DesktopSettings.writes.length, 0);
        number.adjusted(8);
        compare(DesktopSettings.writes[0].key, "gapsIn");
        const monitors = DesktopSettings.monitors;
        DesktopSettings.monitors = [
            {
                name: "eDP-1",
                width: 1920,
                height: 1080,
                scale: 1.5,
                refreshRate: 120,
                availableModes: ["1920x1080@120.00Hz"]
            },
            {
                name: "DP-1",
                width: 2560,
                height: 1440,
                scale: 1,
                refreshRate: 60,
                availableModes: []
            }
        ];
        DesktopSettings.lastRequest = [];
        view.open("displays");
        wait(30);
        const displays = findChild(view, "settingsPage").item;
        displays.edit("eDP-1", 1.25, "");
        compare(DesktopSettings.lastRequest.join("|"), "display-edit|eDP-1|1.25|");
        DesktopSettings.pending = true;
        DesktopSettings.lastRequest = [];
        displays.edit("eDP-1", 1, "");
        compare(DesktopSettings.lastRequest.length, 0);
        DesktopSettings.pending = false;
        displays.edit("disconnected", 1, "");
        compare(DesktopSettings.lastRequest.length, 0);
        displays.layout("extend-left");
        compare(DesktopSettings.lastRequest[0], "display");
        compare(DesktopSettings.lastRequest[1], "extend-left");
        view.open("workflows");
        wait(20);
        const workflow = findChild(view, "settingsPage").item;
        workflow.toggleRole("Browser");
        workflow.save();
        compare(DesktopSettings.lastRequest[0], "save-workflow");
        compare(JSON.parse(DesktopSettings.lastRequest[2])[0], "Browser");
        view.open("maintenance");
        wait(20);
        const maintenance = findChild(view, "settingsPage").item;
        Maintenance.lastAction = "";
        Maintenance.busy = true;
        maintenance.recovery("restart");
        compare(Maintenance.lastAction, "");
        Maintenance.busy = false;
        maintenance.recovery("restart");
        compare(Maintenance.lastAction, "restart");
        DesktopSettings.monitors = monitors;
        DesktopSettings.pending = false;
    }

    function test_palette_tile_selection_and_labels_fit_small_tiles() {
        const tile = createTemporaryObject(paletteTile, test);
        const label = findChild(tile, "paletteName");
        const inUse = findChild(tile, "paletteInUse");
        for (const width of [130, 160, 220]) {
            tile.width = width;
            verify(label.x + label.width <= tile.width);
            verify(label.y + label.height < inUse.y);
            verify(inUse.y + inUse.height <= tile.height);
        }
        verify(!inUse.visible);
        tile.selected = true;
        verify(inUse.visible);
        compare(tile.border.width, 2);
        tile.selected = false;
        compare(tile.border.width, 1);
    }
    Component {
        id: paletteTile
        NacrePaletteTile {
            width: 160
            entry: ({
                    name: "Olive supporting color",
                    surface: "232315",
                    swatches: ["929365", "5ca19a", "839a79"]
                })
        }
    }

    function test_palette_rows_fit_the_available_width() {
        const prior = NacreWallpapers.palettePresets;
        NacreWallpapers.palettePresets = Array.from({
            "length": 12
        }, (_, i) => {
            return ({
                    "id": "test" + i,
                    "name": "Color " + i,
                    "group": "vivid",
                    "surface": "102030",
                    "swatches": ["aabbcc", "ccbbaa", "abcabc"]
                });
        });
        const view = createTemporaryObject(settings, test);
        view.open("appearance");
        wait(30);
        const page = findChild(view, "settingsPage").item;
        const flow = findChild(page, "paletteOptions");
        verify(flow);
        const tiles = flow.children.filter(child => {
            return child.objectName === "paletteColorTile";
        });
        compare(tiles.length, 12);
        for (const tile of tiles)
            verify(tile.x + tile.width <= flow.width + 0.01);
        const columns = Math.max(1, Math.min(4, Math.floor((flow.width + 10) / 170)));
        compare(tiles.filter(tile => {
            return tile.y === tiles[0].y;
        }).length, columns);
        NacreWallpapers.palettePresets = prior;
    }

    function test_wallpaper_palette_options_fit_and_select_an_accent() {
        const prior = NacreWallpapers.paletteOptions;
        const preferences = NacreWallpapers.preferences;
        NacreWallpapers.paletteOptions = ["aabbcc", "ccbbaa", "bbccdd", "ccbbdd", "ddbbcc"].map((accent, i) => ({
                    accent: accent,
                    name: "Image color " + i,
                    surface: "102030",
                    swatches: [accent, "ccbbaa", "abcabc"]
                }));
        const view = createTemporaryObject(settings, test);
        view.open("appearance");
        wait(30);
        const flow = findChild(findChild(view, "settingsPage").item, "wallpaperPaletteOptions");
        verify(flow);
        const tiles = flow.children.filter(child => child.objectName === "wallpaperPaletteTile");
        compare(tiles.length, 5);
        for (const tile of tiles)
            verify(tile.x + tile.width <= flow.width + 0.01);
        tiles[2].choose();
        compare(NacreWallpapers.preferences.paletteAccent, "bbccdd");
        NacreWallpapers.paletteOptions = prior;
        NacreWallpapers.preferences = preferences;
    }

    function test_palette_harmony_is_optional_and_keeps_accent_preference() {
        const prior = NacreWallpapers.preferences;
        NacreWallpapers.preferences = {
            palettePreset: "wallpaper",
            paletteAccent: "aabbcc"
        };
        const view = createTemporaryObject(settings, test);
        view.open("appearance");
        wait(30);
        const page = findChild(view, "settingsPage").item;
        const natural = findChild(page, "naturalPaletteButton");
        const harmony = findChild(page, "harmonyPaletteButton");
        verify(natural.selected);
        verify(!harmony.selected);
        harmony.clicked();
        verify(harmony.selected);
        compare(NacreWallpapers.preferences.paletteAccent, "aabbcc");
        natural.clicked();
        verify(natural.selected);
        NacreWallpapers.preferences = Object.assign({}, NacreWallpapers.preferences, {
            palettePreset: "ocean"
        });
        verify(!harmony.enabled);
        NacreWallpapers.preferences = prior;
    }

    function test_shared_wheel_glides_and_page_switch_resets_position() {
        const view = createTemporaryObject(settings, test);
        wait(30);
        const scroll = findChild(view, "settingsScroll");
        verify(scroll.contentHeight > scroll.height);
        const limit = scroll.contentHeight - scroll.height;
        mouseWheel(scroll, 600, 80, 0, -120);
        wait(100);
        verify(scroll.contentY > 0);
        wait(220);
        verify(Math.abs(scroll.contentY - Math.min(limit, 600)) < 2);
        mouseWheel(scroll, 600, 80, 0, -120);
        wait(20);
        view.open("maintenance");
        wait(300);
        compare(scroll.contentY, 0);
    }

    function test_all_device_and_lock_pages_load_without_warnings() {
        const view = createTemporaryObject(settings, test);
        wait(20);
        for (const page of ["appearance", "displays", "sound", "network", "bluetooth", "notifications", "workflows", "lock", "time", "ai", "maintenance"]) {
            view.open(page);
            wait(20);
            verify(findChild(view, "settingsPage").item, page);
        }
    }

    function test_lock_preview_contents_belong_to_the_window() {
        const view = createTemporaryObject(lockPreview, test);
        verify(view);
        wait(30);
        const window = findChild(view, "lockPreviewWindow");
        const content = findChild(view, "lockPreviewContent");
        verify(window);
        verify(content);
        compare(content.parent, window);
        compare(content.width, window.width);
        compare(content.height, window.height);
        compare(findChild(view, "lockPreviewHour").text, "02");
        verify(window.children.length >= 3);
        window.widgetData = {
            "count": 1,
            "notifications": [
                {
                    "app": "Mail"
                }
            ],
            "weather": {
                "location": "Test city",
                "temperature": "10°C",
                "description": "Overcast"
            }
        };
        wait(20);
        verify(content.visible);
    }

    Component {
        id: audioDevice
        QtObject {
            property bool ready: true
            property bool isSink: false
            property bool isStream: false
            property string description: "A long audio device name ".repeat(10)
            property string nickname: ""
            property string name: "fixture"
            property QtObject audio: QtObject {
                property real volume: .6
                property bool muted: true
            }
        }
    }
    Component {
        id: audioRow
        NacreAudioNode {
            width: 600
        }
    }
    function test_audio_preserves_mute_and_ignores_removed_or_unready_targets() {
        const node = createTemporaryObject(audioDevice, test);
        Pipewire.nodes = {
            values: [node]
        };
        Pipewire.preferredDefaultAudioSource = null;
        const row = createTemporaryObject(audioRow, test, {
            node: node
        });
        wait(20);
        compare(node.audio.volume, .6);
        verify(node.audio.muted);
        compare(Pipewire.preferredDefaultAudioSource, null);
        row.chooseDefault();
        compare(Pipewire.preferredDefaultAudioSource, node);
        verify(node.audio.muted);
        const slider = findChild(row, "audioVolume");
        slider.progress = .9;
        compare(node.audio.volume, .6);
        slider.keyboardRequested(.4);
        compare(node.audio.volume, .4);
        slider.dragBegan();
        Pipewire.nodes = {
            values: []
        };
        slider.dragEnded(.8);
        row.toggleMute();
        compare(node.audio.volume, .4);
        verify(node.audio.muted);
        Pipewire.nodes = {
            values: [node]
        };
        node.ready = false;
        row.setVolume(node, .9);
        compare(node.audio.volume, .4);
        node.ready = true;
        row.toggleMute();
        verify(!node.audio.muted);
        Pipewire.nodes = {
            values: []
        };
        Pipewire.preferredDefaultAudioSource = null;
    }
    function test_connections_are_explicit_busy_and_stale_safe() {
        DeviceActions.requests = [];
        DeviceActions.connectedSSID = "";
        const view = createTemporaryObject(settings, test);
        const network = {
            ssid: "Fixture network",
            strength: 80,
            active: false
        };
        NacreNetwork.visibleNetworks = [network];
        view.open("network");
        wait(20);
        const wifi = findChild(view, "settingsPage").item;
        compare(DeviceActions.requests.length, 0);
        compare(DeviceActions.connectedSSID, "");
        DeviceActions.busy = true;
        wifi.connect(network);
        wifi.request("wifi-radio", "off");
        compare(DeviceActions.requests.length, 0);
        compare(DeviceActions.connectedSSID, "");
        DeviceActions.busy = false;
        wifi.connect(network);
        compare(DeviceActions.connectedSSID, "Fixture network");
        NacreNetwork.visibleNetworks = [];
        DeviceActions.connectedSSID = "";
        wifi.connect(network);
        compare(DeviceActions.connectedSSID, "");
        const device = {
            name: "Headphones",
            alias: "",
            address: "00:11:22:33:44:55",
            connected: false,
            paired: true,
            trusted: false
        };
        NacreBluetooth.devices = [device];
        view.open("bluetooth");
        wait(20);
        const bt = findChild(view, "settingsPage").item;
        compare(DeviceActions.requests.length, 0);
        bt.power();
        compare(DeviceActions.requests[0].join("|"), "bluetooth-power|on");
        bt.deviceAction(device, "connect");
        compare(DeviceActions.requests[1].join("|"), "bluetooth-connect|00:11:22:33:44:55");
        bt.deviceAction(device, "remove");
        DeviceActions.busy = true;
        bt.deviceAction(device, "trust");
        DeviceActions.busy = false;
        NacreBluetooth.devices = [];
        bt.deviceAction(device, "trust");
        compare(DeviceActions.requests.length, 2);
    }
    function test_history_requires_confirmation_and_preserves_long_card_bounds() {
        NacreNotifs.list = [
            {
                key: "fixture",
                appIcon: "",
                appName: "Mail",
                summary: "A long message ".repeat(30),
                body: "Details ".repeat(40),
                image: "",
                timeStr: "Now",
                urgency: 1,
                hovered: false
            }
        ];
        NacreNotifs.clears = 0;
        NacreNotifs.dismissed = "";
        const view = createTemporaryObject(settings, test);
        view.open("notifications");
        wait(20);
        const page = findChild(view, "settingsPage").item;
        compare(NacreNotifs.clears, 0);
        const clear = findChild(page, "requestClearHistory");
        clear.clicked();
        compare(NacreNotifs.clears, 0);
        verify(page.confirmClear);
        clear.clicked();
        compare(NacreNotifs.clears, 1);
        verify(!page.confirmClear);
        const summary = findChild(page, "noticeSummary");
        verify(summary.width <= page.width);
        const dismiss = findChild(page, "noticeDismiss");
        dismiss.activated();
        compare(NacreNotifs.dismissed, "fixture");
        NacreNotifs.list = [];
    }
    function test_time_weather_and_assistant_writes_are_user_only() {
        TimezoneSettings.changes = [];
        SidebarChat.choices = [];
        DesktopSettings.writes = [];
        AppLaunch.commands = [];
        DesktopActions.actions = [];
        const view = createTemporaryObject(settings, test);
        view.open("time");
        wait(20);
        const time = findChild(view, "settingsPage").item;
        compare(TimezoneSettings.changes.length, 0);
        TimezoneSettings.busy = true;
        time.change("manual", "Europe/Oslo");
        compare(TimezoneSettings.changes.length, 0);
        TimezoneSettings.busy = false;
        time.change("confirm", "Europe/Oslo");
        compare(TimezoneSettings.changes[0].join("|"), "confirm|Europe/Oslo");
        time.change("invalid", "Europe/Oslo");
        compare(TimezoneSettings.changes.length, 1);
        view.open("lock");
        wait(20);
        const lock = findChild(view, "settingsPage").item;
        compare(DesktopSettings.writes.length, 0);
        compare(AppLaunch.commands.length, 0);
        compare(DesktopActions.actions.length, 0);
        lock.saveWeather(" Oslo ");
        compare(DesktopSettings.writes[0].key, "weatherLocation");
        compare(DesktopSettings.writes[0].value, "Oslo");
        view.open("ai");
        wait(20);
        const ai = findChild(view, "settingsPage").item;
        compare(SidebarChat.choices.length, 0);
        ai.chooseProvider("invalid");
        compare(SidebarChat.choices.length, 0);
        ai.chooseProvider("claude");
        compare(SidebarChat.choices[0], "claude");
        compare(SidebarChat.provider, "codex");
        compare(AppLaunch.commands.length, 0);
        SidebarChat.defaultProvider = "codex";
    }

    name: "SettingsControlCenter"
    width: 1120
    height: 300
    visible: true
    when: windowShown

    Component {
        id: settings

        NacreSettings {
            width: 1120
            height: 300
            page: "desktop"
        }
    }

    Component {
        id: lockPreview

        LockPreview {
            width: 1280
            height: 800
        }
    }
}
