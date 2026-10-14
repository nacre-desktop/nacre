import QtQuick
import Quickshell.Services.UPower
import qs.services

QtObject {
    id: root
    required property var screen
    signal detailsRequested(string section)
    readonly property string onText: NacreI18n.tr("controlcenter.control-center.detail.on", "On")
    readonly property string offText: NacreI18n.tr("controlcenter.control-center.detail.off", "Off")
    readonly property string profile: PowerProfiles.profile === PowerProfile.PowerSaver ? NacreI18n.tr("popouts.battery-popup.label.power-saver", "Power saver") : PowerProfiles.profile === PowerProfile.Performance ? NacreI18n.tr("popouts.battery-popup.label.performance", "Performance") : NacreI18n.tr("popouts.battery-popup.label.balanced", "Balanced")
    readonly property var tiles: [
        {
            id: "network",
            name: NacreNetwork.wiredConnected ? NacreI18n.tr("controlcenter.network.ethernet", "Ethernet") : NacreI18n.tr("controlcenter.control-center.label.wi-fi", "Wi-Fi"),
            icon: NacreNetwork.wiredConnected ? "lan" : "wifi",
            status: NacreNetwork.wiredConnected ? NacreI18n.tr("controlcenter.network.wired-connected", "Connected by cable") : NacreNetwork.active?.ssid || (NacreNetwork.wifiEnabled ? NacreI18n.tr("controlcenter.control-center.detail.not-connected", "Not connected") : offText),
            on: NacreNetwork.wiredConnected || NacreNetwork.wifiEnabled,
            available: NacreNetwork.wiredConnected || (NacreNetwork.wifiAvailable && !NacreControlActions.busy)
        },
        {
            id: "bluetooth",
            name: NacreI18n.tr("controlcenter.control-center.label.bluetooth", "Bluetooth"),
            icon: "bluetooth",
            status: NacreBluetooth.powered ? NacreBluetooth.devices.filter(device => device.connected).map(device => device.alias || device.name).join(", ") || onText : offText,
            on: NacreBluetooth.powered,
            available: NacreBluetooth.available && !NacreControlActions.busy
        },
        {
            id: "battery",
            name: NacreI18n.tr("controlcenter.overview.power-mode", "Power mode"),
            icon: PowerProfiles.profile === PowerProfile.PowerSaver ? "eco" : PowerProfiles.profile === PowerProfile.Performance ? "bolt" : "balance",
            status: NacreControlTools.data.powerProfilesSupported ? profile : NacreI18n.tr("popouts.battery-popup.text.power-profiles-unavailable", "Power profiles unavailable"),
            on: false,
            available: NacreControlTools.data.powerProfilesSupported === true
        },
        {
            id: "night",
            name: NacreI18n.tr("controlcenter.control-center.label.night-light", "Night light"),
            icon: "nightlight",
            status: !NacreControlTools.data.nightLightSupported ? NacreControlTools.data.nightLightReason || NacreI18n.tr("ui.compositor.unavailable", "Unavailable") : NacreControlTools.data.nightLightExternal ? NacreI18n.tr("controlcenter.control-center.detail.external", "External") : NacreControlTools.data.nightLightEnabled ? NacreI18n.tr("controlcenter.control-center.detail.warm", "Warm") : offText,
            on: NacreControlTools.data.nightLightEnabled === true,
            available: NacreControlTools.data.nightLightSupported === true && !NacreControlTools.data.nightLightExternal && !NacreControlTools.busy
        },
        {
            id: "dnd",
            name: NacreI18n.tr("controlcenter.overview.do-not-disturb", "Do Not Disturb"),
            icon: "do_not_disturb_on",
            status: NacreNotifs.dnd ? onText : offText,
            on: NacreNotifs.dnd,
            available: true
        },
        {
            id: "awake",
            name: NacreI18n.tr("controlcenter.overview.stay-awake", "Stay awake"),
            icon: "coffee",
            status: NacreMenu.snapshot.stayAwake ? onText : offText,
            on: !!NacreMenu.snapshot.stayAwake,
            available: true
        }
    ].concat(optional)
    readonly property var optional: [
        {
            id: "airplane",
            name: NacreI18n.tr("controlcenter.overview.airplane", "Airplane mode"),
            icon: "flight",
            status: !NacreNetwork.wifiEnabled && !NacreBluetooth.powered ? onText : offText,
            on: !NacreNetwork.wifiEnabled && !NacreBluetooth.powered,
            available: !NacreControlActions.busy && (NacreNetwork.wifiAvailable || NacreBluetooth.available)
        },
        {
            id: "vpn",
            name: NacreI18n.tr("controlcenter.overview.vpn", "VPN"),
            icon: "vpn_key",
            status: NacreControlPreferences.optionalTiles.includes("vpn") ? NacreVpn.connected[0]?.name || NacreI18n.tr("controlcenter.control-center.detail.off", "Off") : "",
            on: NacreControlPreferences.optionalTiles.includes("vpn") && NacreVpn.connected.length > 0,
            available: NacreControlPreferences.optionalTiles.includes("vpn") && NacreVpn.profiles.length > 0 && !NacreVpn.busy
        },
        {
            id: "record",
            name: NacreI18n.tr("controlcenter.overview.recorder", "Screen recorder"),
            icon: "videocam",
            status: NacreMenu.snapshot.capabilities?.record ? (NacreMenu.snapshot.recording ? onText : offText) : NacreI18n.tr("ui.compositor.unavailable", "Unavailable"),
            on: !!NacreMenu.snapshot.recording,
            available: NacreMenu.snapshot.capabilities?.record === true
        },
        {
            id: "mic",
            name: NacreI18n.tr("controlcenter.overview.mic-mute", "Mic mute"),
            icon: NacreAudio.micMuted ? "mic_off" : "mic",
            status: NacreAudio.micMuted ? onText : offText,
            on: NacreAudio.micMuted,
            available: NacreAudio.micAvailable
        }
    ]
    function toggle(id) {
        if (!supported(id) || !find(id)?.available)
            return;
        if (id === "network") {
            if (NacreNetwork.wiredConnected)
                root.detailsRequested("network");
            else
                NacreControlActions.request(["wifi-radio", NacreNetwork.wifiEnabled ? "off" : "on"]);
        } else if (id === "bluetooth")
            NacreControlActions.request(["bluetooth-power", NacreBluetooth.powered ? "off" : "on"]);
        else if (id === "battery")
            root.detailsRequested("battery");
        else if (id === "night")
            NacreControlTools.toggleNightLight();
        else if (id === "dnd")
            DesktopSettings.set("dnd", !NacreNotifs.dnd);
        else if (id === "awake")
            NacreMenu.work("tool", "stay-awake");
        else if (id === "record")
            NacreMenu.work("tool", NacreMenu.snapshot.recording ? "record-stop" : "record-start");
        else if (id === "vpn")
            NacreVpn.toggle();
        else if (id === "mic")
            NacreAudio.toggleMic();
        else if (id === "airplane")
            NacreControlActions.setAirplane(!NacreNetwork.wifiEnabled && !NacreBluetooth.powered ? false : true);
        else if (id === "screenshot" || id === "picker")
            NacreControlTools.capture(id === "picker" ? "color-picker" : "screenshot");
        else if (id === "clipboard")
            NacrePanelState.openMode("clipboard");
        else if (id === "wallpaper")
            NacrePanelState.openSettings("appearance");
        else if (id === "next-wallpaper")
            NacreWallpapers.advanceRotation(true);
        else if (id === "updates")
            root.detailsRequested("updates");
        else if (id === "displays")
            NacrePanelState.openSettings("displays");
    }

    readonly property var extras: [
        {
            id: "volume",
            name: NacreI18n.tr("controlcenter.control-levels.label.volume", "Volume"),
            icon: "volume_up"
        },
        {
            id: "brightness",
            name: NacreI18n.tr("controlcenter.levels.brightness", "Brightness"),
            icon: "brightness_6"
        },
        {
            id: "mic-volume",
            name: NacreI18n.tr("controlcenter.catalog.mic-volume", "Microphone volume"),
            icon: "mic"
        },
        {
            id: "keyboard",
            name: NacreI18n.tr("controlcenter.displays.keyboard", "Keyboard backlight"),
            icon: "keyboard"
        },
        {
            id: "appearance",
            name: NacreI18n.tr("controlcenter.overview.appearance", "Appearance"),
            icon: "palette"
        },
        {
            id: "screenshot",
            name: NacreI18n.tr("controlcenter.catalog.screenshot", "Screenshot"),
            icon: "screenshot",
            status: NacreI18n.tr("controlcenter.catalog.select-area", "Select an area"),
            on: false,
            available: true
        },
        {
            id: "picker",
            name: NacreI18n.tr("controlcenter.catalog.picker", "Color picker"),
            icon: "colorize",
            status: NacreI18n.tr("controlcenter.catalog.pick-color", "Pick from the screen"),
            on: false,
            available: true
        },
        {
            id: "clipboard",
            name: NacreI18n.tr("controlcenter.catalog.clipboard", "Clipboard"),
            icon: "content_paste",
            status: NacreI18n.tr("controlcenter.catalog.open-history", "Open history"),
            on: false,
            available: true
        },
        {
            id: "wallpaper",
            name: NacreI18n.tr("controlcenter.catalog.wallpaper", "Wallpaper"),
            icon: "wallpaper",
            status: NacreI18n.tr("controlcenter.catalog.change-wallpaper", "Choose a background"),
            on: false,
            available: true
        },
        {
            id: "next-wallpaper",
            name: NacreI18n.tr("controlcenter.catalog.next-wallpaper", "Next wallpaper"),
            icon: "skip_next",
            status: NacreI18n.tr("controlcenter.catalog.next-background", "Change the background"),
            on: false,
            available: true
        },
        {
            id: "updates",
            name: NacreI18n.tr("status.peek.updates", "Updates"),
            icon: "system_update",
            status: NacreI18n.tr("controlcenter.catalog.check-updates", "Check for updates"),
            on: false,
            available: true
        },
        {
            id: "displays",
            name: NacreI18n.tr("controlcenter.catalog.displays", "Display settings"),
            icon: "desktop_windows",
            status: NacreI18n.tr("controlcenter.catalog.arrange-displays", "Arrange your monitors"),
            on: false,
            available: true
        }
    ]
    readonly property var entries: (tiles || []).filter(tile => tile.id !== "theme").concat(extras || [])
    function supported(id) {
        if (id === "night")
            return NacreControlTools.data.nightLightSupported === true;
        if (id === "brightness")
            return NacreBrightness.getMonitorForScreen(screen)?.available === true;
        if (id === "keyboard")
            return NacreKeyboardLight.available;
        if (id === "mic-volume" || id === "mic")
            return NacreAudio.micAvailable;
        if (id === "screenshot")
            return NacreControlTools.data.screenshot === true;
        if (id === "picker")
            return NacreControlTools.data.colorPicker === true;
        if (id === "record")
            return NacreMenu.snapshot.capabilities?.record === true;
        return true;
    }
    function find(id) {
        return (entries || []).find(tile => tile.id === id);
    }
}
