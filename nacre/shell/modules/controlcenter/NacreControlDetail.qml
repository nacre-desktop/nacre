import QtQuick
import qs.widgets
import qs.services
import qs.modules.bar.popouts as Details

Column {
    id: root
    property bool active: true
    required property string section
    required property var screen
    readonly property string description: ({
            night: !NacreControlTools.data.nightLightSupported ? NacreControlTools.data.nightLightReason || NacreI18n.tr("controlcenter.details.night-unavailable", "Night light is unavailable on this compositor.") : NacreControlTools.data.nightLightExternal ? NacreI18n.tr("controlcenter.details.night-external", "Night light is managed by another application.") : NacreI18n.tr("controlcenter.details.night", "Warmer display colors for the evening."),
            dnd: NacreI18n.tr("controlcenter.details.dnd", "Silence notification pop-ups. Your notifications remain available from the bell in the bar."),
            awake: NacreI18n.tr("controlcenter.details.awake", "Keep the session awake until you turn this off. Your usual idle behavior returns when you switch it off."),
            theme: NacreI18n.tr("controlcenter.details.theme", "Use light or dark surfaces with your current Orient palette."),
            airplane: NacreI18n.tr("controlcenter.details.airplane", "Turn off Wi-Fi and Bluetooth together. Turning airplane mode off restores their previous state. Wired connections stay available."),
            record: NacreMenu.snapshot.capabilities?.record ? NacreI18n.tr("controlcenter.details.record", "Start or stop a recording with the existing screen recorder. Select a region when prompted.") : NacreI18n.tr("controlcenter.details.record-unavailable", "Screen recording requires a supported capture session and an installed recorder.")
        })[section] || ""
    spacing: 20
    NacreText {
        width: parent.width
        text: root.description
        wrapMode: Text.Wrap
        font.pointSize: 12
        color: NacreTokens.mutedInk
    }
    Loader {
        width: parent.width
        active: root.section === "mic"
        sourceComponent: Details.NacreSoundPopup {
            active: root.active
        }
    }
    Loader {
        width: parent.width
        active: root.section === "vpn"
        sourceComponent: NacreControlVpn {}
    }
    Row {
        width: parent.width
        spacing: 10
        visible: root.section === "theme"
        ActionButton {
            text: NacreI18n.tr("controlcenter.overview.light", "Light")
            icon: "light_mode"
            selected: NacreTokens.light
            onClicked: NacreColours.setMode("light")
        }
        ActionButton {
            text: NacreI18n.tr("controlcenter.overview.dark", "Dark")
            icon: "dark_mode"
            selected: !NacreTokens.light
            onClicked: NacreColours.setMode("dark")
        }
    }
    ActionButton {
        visible: ["night", "dnd", "awake", "airplane", "record"].includes(root.section)
        enabled: root.section === "night" ? NacreControlTools.data.nightLightSupported === true && !NacreControlTools.data.nightLightExternal && !NacreControlTools.busy : root.section === "record" ? NacreMenu.snapshot.capabilities?.record === true : root.section === "airplane" ? !NacreControlActions.busy : true
        readonly property bool selectedState: root.section === "dnd" ? NacreNotifs.dnd : root.section === "night" ? NacreControlTools.data.nightLightEnabled === true : root.section === "awake" ? !!NacreMenu.snapshot.stayAwake : root.section === "record" ? !!NacreMenu.snapshot.recording : !NacreNetwork.wifiEnabled && !NacreBluetooth.powered
        text: selectedState ? NacreI18n.tr("popouts.network-popup.text.turn-off", "Turn off") : NacreI18n.tr("popouts.network-popup.text.turn-on", "Turn on")
        onClicked: {
            if (root.section === "dnd")
                DesktopSettings.set("dnd", !NacreNotifs.dnd);
            else if (root.section === "night")
                NacreControlTools.toggleNightLight();
            else if (root.section === "awake")
                NacreMenu.work("tool", "stay-awake");
            else if (root.section === "record")
                NacreMenu.work("tool", NacreMenu.snapshot.recording ? "record-stop" : "record-start");
            else if (root.section === "airplane")
                NacreControlActions.setAirplane(!selectedState);
        }
    }
    NacreText {
        width: parent.width
        visible: text !== ""
        text: NacreControlTools.error || NacreMenu.message || NacreControlActions.message || ""
        color: NacreTokens.mutedInk
        wrapMode: Text.Wrap
        font.pointSize: 10
    }
}
