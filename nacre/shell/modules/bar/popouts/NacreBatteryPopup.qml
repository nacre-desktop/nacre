import QtQuick
import Quickshell.Services.UPower
import qs.widgets
import qs.services
import qs.config

Column {
    id: root
    property var device: UPower.displayDevice
    property bool profilesEnabled: true
    property bool showSummary: true
    readonly property bool available: !!device && device.ready && device.isPresent !== false && device.isLaptopBattery && Number.isFinite(device.percentage) && device.percentage >= 0 && device.percentage <= 1
    readonly property int percent: available ? Math.round(device.percentage * 100) : -1
    readonly property string estimate: available ? formatSeconds(UPower.onBattery ? device.timeToEmpty : device.timeToFull, UPower.onBattery ? NacreI18n.tr("battery.remaining-unavailable", "Remaining time unavailable") : NacreI18n.tr("battery.connected-power", "Connected to power")) : NacreI18n.tr("battery.unavailable", "Battery unavailable")
    readonly property string profileName: PowerProfiles.profile === PowerProfile.PowerSaver ? NacreI18n.tr("popouts.battery-popup.label.power-saver", "Power saver") : PowerProfiles.profile === PowerProfile.Performance ? NacreI18n.tr("popouts.battery-popup.label.performance", "Performance") : NacreI18n.tr("popouts.battery-popup.label.balanced", "Balanced")
    spacing: NacreAppearance.spacing.normal
    width: NacreBar.sizes.batteryWidth
    function formatSeconds(seconds, fallback) {
        if (!Number.isFinite(seconds) || seconds <= 0)
            return fallback;
        const minutes = Math.max(1, Math.ceil(seconds / 60));
        if (minutes >= 60)
            return NacreI18n.tr(UPower.onBattery ? "battery.remaining-hours" : "battery.until-full-hours", UPower.onBattery ? "{hours}h {minutes}m remaining" : "{hours}h {minutes}m until full", {
                hours: Math.floor(minutes / 60),
                minutes: minutes % 60
            });
        return NacreI18n.tr(UPower.onBattery ? "battery.remaining-minutes" : "battery.until-full-minutes", UPower.onBattery ? "{minutes}m remaining" : "{minutes}m until full", {
            minutes: minutes
        });
    }
    function chooseProfile(profile) {
        if (!profilesEnabled || ![PowerProfile.PowerSaver, PowerProfile.Balanced, PowerProfile.Performance].includes(profile))
            return false;
        if (profile === PowerProfile.Performance && !PowerProfiles.hasPerformanceProfile)
            return false;
        PowerProfiles.profile = profile;
        return true;
    }
    NacreText {
        visible: root.showSummary
        objectName: "nacreBatteryPercent"
        width: root.width
        text: root.available ? NacreI18n.number(root.percent) + "%" : "—"
        horizontalAlignment: Text.AlignHCenter
        font.family: NacreAppearance.font.family.mono
        font.pointSize: NacreAppearance.font.size.large
    }
    Rectangle {
        visible: root.showSummary
        width: root.width
        height: 8
        radius: 4
        color: NacreColours.palette.m3surfaceContainer
        Rectangle {
            width: parent.width * (root.available ? root.percent / 100 : 0)
            height: parent.height
            radius: 4
            color: root.available && root.percent <= 15 && UPower.onBattery ? NacreColours.palette.m3error : NacreColours.palette.m3primary
        }
    }
    NacreText {
        visible: root.showSummary
        objectName: "nacreBatteryEstimate"
        width: root.width
        text: root.estimate
        wrapMode: Text.WordWrap
        horizontalAlignment: Text.AlignHCenter
        font.pointSize: 11
        color: NacreColours.palette.m3onSurfaceVariant
    }
    NacreText {
        width: root.width
        visible: root.showSummary
        text: root.profilesEnabled ? root.profileName : NacreI18n.tr("popouts.battery-popup.text.power-profiles-unavailable", "Power profiles unavailable")
        horizontalAlignment: Text.AlignHCenter
        font.pointSize: 11
    }
    Row {
        width: root.width
        spacing: 8
        ProfileButton {
            profile: PowerProfile.PowerSaver
            icon: "eco"
            label: NacreI18n.tr("popouts.battery-popup.label.power-saver", "Power saver")
        }
        ProfileButton {
            profile: PowerProfile.Balanced
            icon: "balance"
            label: NacreI18n.tr("popouts.battery-popup.label.balanced", "Balanced")
        }
        ProfileButton {
            profile: PowerProfile.Performance
            icon: "bolt"
            label: NacreI18n.tr("popouts.battery-popup.label.performance", "Performance")
        }
    }
    NacreText {
        width: root.width
        visible: text !== ""
        text: PowerProfiles.degradationReason || ""
        wrapMode: Text.WordWrap
        font.pointSize: 10
        color: NacreColours.palette.m3onSurfaceVariant
    }
    component ProfileButton: NacreSurface {
        required property int profile
        objectName: profile === PowerProfile.PowerSaver ? "nacreProfileSaver" : profile === PowerProfile.Performance ? "nacreProfilePerformance" : "nacreProfileBalanced"
        required property string icon
        required property string label
        readonly property bool selected: profile === PowerProfiles.profile
        width: (root.width - 16) / 3
        height: 72
        radius: 19
        enabled: root.profilesEnabled && (profile !== PowerProfile.Performance || PowerProfiles.hasPerformanceProfile)
        opacity: enabled ? 1 : 0.45
        color: selected ? NacreColours.palette.m3primaryContainer : NacreColours.palette.m3surfaceContainer
        Column {
            anchors.centerIn: parent
            width: parent.width - 8
            spacing: 6
            NacreIcon {
                anchors.horizontalCenter: parent.horizontalCenter
                text: parent.parent.icon
                font.pointSize: 18
                color: parent.parent.selected ? NacreColours.palette.m3onPrimaryContainer : NacreTokens.mutedInk
            }
            NacreText {
                width: parent.width
                text: parent.parent.label
                horizontalAlignment: Text.AlignHCenter
                font.pointSize: 9
                wrapMode: Text.Wrap
                color: parent.parent.selected ? NacreColours.palette.m3onPrimaryContainer : NacreTokens.ink
            }
        }
        NacreInteraction {
            accessibleName: parent.label
            function onClicked() {
                root.chooseProfile(parent.profile);
            }
        }
    }
}
