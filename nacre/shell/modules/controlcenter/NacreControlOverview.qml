import QtQuick
import QtQuick.Layouts
import Quickshell.Services.UPower
import qs.widgets
import qs.services

Column {
    id: root
    required property var screen
    property bool active: true
    signal detailsRequested(string section)
    readonly property var battery: UPower.displayDevice
    readonly property bool hasBattery: !!battery && battery.ready && battery.isPresent !== false && battery.isLaptopBattery
    readonly property string onText: NacreI18n.tr("controlcenter.control-center.detail.on", "On")
    readonly property string offText: NacreI18n.tr("controlcenter.control-center.detail.off", "Off")
    readonly property string profile: PowerProfiles.profile === PowerProfile.PowerSaver ? NacreI18n.tr("popouts.battery-popup.label.power-saver", "Power saver") : PowerProfiles.profile === PowerProfile.Performance ? NacreI18n.tr("popouts.battery-popup.label.performance", "Performance") : NacreI18n.tr("popouts.battery-popup.label.balanced", "Balanced")
    readonly property var tileIds: ["network", "bluetooth", "battery", "night", "dnd", "awake"].concat(NacreControlPreferences.optionalTiles)
    readonly property var tiles: [
        {
            id: "network",
            name: NacreI18n.tr("controlcenter.control-center.label.wi-fi", "Wi-Fi"),
            icon: "wifi",
            status: NacreNetwork.active?.ssid || (NacreNetwork.wifiEnabled ? NacreI18n.tr("controlcenter.control-center.detail.not-connected", "Not connected") : offText),
            on: NacreNetwork.wifiEnabled,
            available: NacreNetwork.wifiAvailable && !NacreControlActions.busy
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
            icon: "eco",
            status: NacreControlTools.data.powerProfilesSupported ? profile : NacreI18n.tr("popouts.battery-popup.text.power-profiles-unavailable", "Power profiles unavailable"),
            on: PowerProfiles.profile === PowerProfile.PowerSaver,
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
        },
        {
            id: "theme",
            name: NacreI18n.tr("controlcenter.overview.appearance", "Appearance"),
            icon: NacreTokens.light ? "light_mode" : "dark_mode",
            status: NacreTokens.light ? NacreI18n.tr("controlcenter.overview.light", "Light") : NacreI18n.tr("controlcenter.overview.dark", "Dark"),
            on: false,
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
    Component.onCompleted: if (NacreControlPreferences.optionalTiles.includes("vpn"))
        NacreVpn.refresh()
    spacing: 18
    function toggle(id) {
        if (!tiles.find(tile => tile.id === id)?.available)
            return;
        if (id === "network")
            NacreControlActions.request(["wifi-radio", NacreNetwork.wifiEnabled ? "off" : "on"]);
        else if (id === "bluetooth")
            NacreControlActions.request(["bluetooth-power", NacreBluetooth.powered ? "off" : "on"]);
        else if (id === "battery")
            PowerProfiles.profile = PowerProfiles.profile === PowerProfile.PowerSaver ? PowerProfile.Balanced : PowerProfile.PowerSaver;
        else if (id === "night")
            NacreControlTools.toggleNightLight();
        else if (id === "dnd")
            DesktopSettings.set("dnd", !NacreNotifs.dnd);
        else if (id === "theme")
            NacreColours.setMode(NacreTokens.light ? "dark" : "light");
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
    }
    NacreControlLevels {
        width: parent.width
        monitor: NacreBrightness.getMonitorForScreen(root.screen)
        onDetailsRequested: section => root.detailsRequested(section)
    }
    GridLayout {
        width: parent.width
        columns: 2
        columnSpacing: 8
        rowSpacing: 8
        Repeater {
            model: root.tileIds
            delegate: NacreControlTile {
                required property string modelData
                readonly property var tile: root.tiles.find(value => value.id === modelData)
                objectName: "control" + ({
                        network: "Wifi",
                        bluetooth: "Bluetooth",
                        battery: "Power",
                        dnd: "Dnd",
                        night: "NightLight",
                        awake: "Awake",
                        theme: "Theme",
                        airplane: "Airplane",
                        vpn: "Vpn",
                        record: "Record",
                        mic: "Mic"
                    })[modelData]
                Layout.fillWidth: true
                Layout.minimumWidth: 0
                label: tile.name
                icon: tile.icon
                detail: modelData === "night" && NacreCompositor.backendName === "niri" && !tile.available ? NacreI18n.tr("controlcenter.night.niri", "Unavailable on niri") : tile.status
                selected: tile.on
                Layout.minimumHeight: 68
                available: tile.available
                hasDetails: ["network", "bluetooth", "battery"].includes(modelData)
                contextDetails: true
                explanation: modelData === "night" && !tile.available ? NacreControlTools.data.nightLightReason || NacreI18n.tr("controlcenter.details.night-unavailable", "Night light is unavailable on this compositor.") : ""
                onClicked: root.toggle(modelData)
                onDetailsRequested: root.detailsRequested(modelData)
            }
        }
    }
    NacreControlAppearance {
        width: parent.width
    }
    Loader {
        width: parent.width
        active: !!NacrePlayers.active
        sourceComponent: NacreControlMedia {
            active: root.active
        }
    }
    NacreText {
        width: parent.width
        visible: text !== ""
        text: NacreControlTools.error || NacreControlPreferences.error || ""
        font.pointSize: 9
        color: NacreColours.palette.m3error
        wrapMode: Text.Wrap
    }
}
