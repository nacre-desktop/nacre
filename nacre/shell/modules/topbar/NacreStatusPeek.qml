import QtQuick
import Quickshell
import Quickshell.Wayland
import Quickshell.Services.UPower
import qs.widgets
import qs.config
import qs.services

NacreWindow {
    id: root
    required property var output
    required property string kind
    property real center: 0
    readonly property var connected: NacreBluetooth.powered ? NacreBluetooth.devices.filter(device => device.connected) : []
    readonly property var battery: UPower.displayDevice
    readonly property bool batteryAvailable: !!battery && battery.ready && battery.isPresent !== false && battery.isLaptopBattery && Number.isFinite(battery.percentage) && battery.percentage >= 0 && battery.percentage <= 1
    readonly property string profile: PowerProfiles.profile === PowerProfile.PowerSaver ? NacreI18n.tr("popouts.battery-popup.label.power-saver", "Power saver") : PowerProfiles.profile === PowerProfile.Performance ? NacreI18n.tr("popouts.battery-popup.label.performance", "Performance") : NacreI18n.tr("popouts.battery-popup.label.balanced", "Balanced")
    readonly property string title: ({
            audio: NacreI18n.tr("popouts.sound-popup.text.sound", "Sound"),
            network: NacreI18n.tr("controlcenter.control-center.label.wi-fi", "Wi-Fi"),
            bluetooth: NacreI18n.tr("controlcenter.control-center.label.bluetooth", "Bluetooth"),
            battery: NacreI18n.tr("status.peek.battery", "Battery"),
            updates: NacreI18n.tr("status.peek.updates", "Updates"),
            notifications: NacreI18n.tr("ui.header.hoverHint.notifications", "Notifications")
        })[kind] || ""
    readonly property string summary: {
        if (kind === "audio")
            return NacreAudio.available === false ? NacreI18n.tr("popouts.sound-popup.text.output-unavailable", "Output unavailable") : NacreAudio.muted ? NacreI18n.tr("status.peek.muted", "Muted") : NacreI18n.number(Math.round(NacreAudio.volume * 100)) + "%";
        if (kind === "network")
            return NacreNetwork.wifiAvailable === false ? NacreI18n.tr("ui.status-icons.label.no-wi-fi-adapter", "No Wi-Fi adapter") : NacreNetwork.wifiEnabled ? NacreNetwork.active?.ssid || NacreI18n.tr("controlcenter.control-center.detail.not-connected", "Not connected") : NacreI18n.tr("controlcenter.control-center.detail.off", "Off");
        if (kind === "bluetooth")
            return NacreBluetooth.available === false ? NacreI18n.tr("ui.status-icons.label.no-bluetooth-adapter", "No Bluetooth adapter") : NacreBluetooth.powered ? connected.length === 1 ? NacreI18n.tr("status.peek.connected-device", "1 connected device") : connected.length ? NacreI18n.tr("status.peek.connected-devices", "{count} connected devices", {
                count: connected.length
            }) : NacreI18n.tr("status.peek.no-devices", "No connected devices") : NacreI18n.tr("controlcenter.control-center.detail.off", "Off");
        if (kind === "battery")
            return batteryAvailable ? NacreI18n.number(Math.round(battery.percentage * 100)) + "%" : NacreI18n.tr("components.status-icons.label.battery-unavailable", "Battery unavailable");
        if (kind === "updates")
            return Updates.count ? NacreI18n.tr("status.peek.update-count", "{count} updates available", {
                count: Updates.count
            }) : NacreI18n.tr("status.peek.up-to-date", "Up to date");
        return NacreI18n.tr("ui.status-icons.label.value0-notifications", "{value0} notifications", {
            value0: NacreNotifs.retained.length
        });
    }
    readonly property string detail: {
        if (kind === "audio")
            return NacreAudio.sink?.description || "";
        if (kind === "network")
            return NacreNetwork.active ? NacreI18n.tr("status.peek.signal", "Signal {percent}%", {
                percent: NacreI18n.number(NacreNetwork.active.strength || 0)
            }) : "";
        if (kind === "battery" && batteryAvailable) {
            const seconds = UPower.onBattery ? battery.timeToEmpty : battery.timeToFull;
            if (!Number.isFinite(seconds) || seconds <= 0)
                return profile;
            const minutes = Math.ceil(seconds / 60);
            return NacreI18n.tr(UPower.onBattery ? "battery.remaining-hours" : "battery.until-full-hours", UPower.onBattery ? "{hours}h {minutes}m remaining" : "{hours}h {minutes}m until full", {
                hours: Math.floor(minutes / 60),
                minutes: minutes % 60
            }) + " · " + profile;
        }
        return "";
    }
    name: "status-peek"
    screen: output
    anchors.top: true
    anchors.left: true
    implicitWidth: 280
    implicitHeight: textColumn.implicitHeight + 28
    margins.top: NacreFrame.headerHeight + 8
    margins.left: Math.max(8, Math.min((output?.width ?? 800) - implicitWidth - 8, center - implicitWidth / 2))
    visible: !NacrePanelState.hidden && DesktopSettings.data.topEdge !== false
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.exclusionMode: ExclusionMode.Ignore
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
    mask: Region {
        width: 0
        height: 0
    }
    NacreSurface {
        anchors.fill: parent
        radius: 18
        color: NacreTokens.body
        border.width: 1
        border.color: Qt.alpha(NacreTokens.outline, .25)
        Column {
            id: textColumn
            x: 14
            y: 14
            width: parent.width - 28
            spacing: 6
            NacreText {
                text: root.title
                font.pointSize: 10
                font.weight: Font.Medium
                color: NacreTokens.mutedInk
            }
            NacreText {
                width: parent.width
                text: root.summary
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                font.pointSize: 12
            }
            NacreText {
                width: parent.width
                text: root.detail
                visible: text !== ""
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                font.pointSize: 10
                color: NacreTokens.mutedInk
            }
            Repeater {
                model: root.kind === "bluetooth" ? root.connected.slice(0, 4) : []
                delegate: Row {
                    required property var modelData
                    width: textColumn.width
                    spacing: 6
                    NacreText {
                        width: parent.width - 60
                        text: modelData.alias || modelData.name
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                        font.pointSize: 10
                    }
                    NacreIcon {
                        text: "battery_full"
                        font.pointSize: 12
                        visible: modelData.batteryAvailable === true
                        color: NacreTokens.accent
                    }
                    NacreText {
                        text: NacreI18n.number(Math.round((modelData.battery || 0) * 100)) + "%"
                        visible: modelData.batteryAvailable === true
                        font.pointSize: 9
                    }
                }
            }
            NacreText {
                visible: root.kind === "bluetooth" && root.connected.length > 4
                text: NacreI18n.tr("status.peek.more-devices", "{count} more devices", {
                    count: Math.max(0, root.connected.length - 4)
                })
                font.pointSize: 10
                color: NacreTokens.mutedInk
            }
        }
    }
}
