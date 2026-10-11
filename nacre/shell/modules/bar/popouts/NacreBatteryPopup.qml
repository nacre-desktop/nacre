import QtQuick
import Quickshell.Services.UPower
import qs.widgets
import qs.services
import qs.config

Column {
    id: root
    property var device: UPower.displayDevice
    property bool profilesEnabled: true
    readonly property bool available: device.ready && device.isLaptopBattery && Number.isFinite(device.percentage) && device.percentage >= 0 && device.percentage <= 1
    readonly property int percent: available ? Math.round(device.percentage * 100) : -1
    readonly property string estimate: available ? formatSeconds(UPower.onBattery ? device.timeToEmpty : device.timeToFull, UPower.onBattery ? "Remaining time unavailable" : "Connected to power") : "Battery unavailable"
    readonly property string profileName: PowerProfiles.profile === PowerProfile.PowerSaver ? "Power saver" : PowerProfiles.profile === PowerProfile.Performance ? "Performance" : "Balanced"
    spacing: NacreAppearance.spacing.normal
    width: NacreBar.sizes.batteryWidth
    function formatSeconds(seconds, fallback) {
        if (!Number.isFinite(seconds) || seconds <= 0)
            return fallback;
        const minutes = Math.max(1, Math.ceil(seconds / 60));
        return (minutes >= 60 ? Math.floor(minutes / 60) + "h " : "") + minutes % 60 + "m " + (UPower.onBattery ? "remaining" : "until full");
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
        objectName: "nacreBatteryPercent"
        width: root.width
        text: root.available ? root.percent + "%" : "—"
        horizontalAlignment: Text.AlignHCenter
        font.family: NacreAppearance.font.family.mono
        font.pointSize: NacreAppearance.font.size.large
    }
    Rectangle {
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
        text: root.profilesEnabled ? root.profileName : "Power profiles unavailable"
        horizontalAlignment: Text.AlignHCenter
        font.pointSize: 11
    }
    Row {
        width: root.width
        spacing: 8
        ProfileButton {
            profile: PowerProfile.PowerSaver
            icon: "eco"
            label: "Power saver"
        }
        ProfileButton {
            profile: PowerProfile.Balanced
            icon: "balance"
            label: "Balanced"
        }
        ProfileButton {
            profile: PowerProfile.Performance
            icon: "bolt"
            label: "Performance"
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
        height: 38
        radius: 19
        enabled: root.profilesEnabled && (profile !== PowerProfile.Performance || PowerProfiles.hasPerformanceProfile)
        opacity: enabled ? 1 : 0.45
        color: selected ? NacreColours.palette.m3primary : NacreColours.palette.m3surfaceContainer
        NacreIcon {
            anchors.centerIn: parent
            text: parent.icon
            color: parent.selected ? NacreColours.palette.m3onPrimary : NacreColours.palette.m3onSurface
        }
        NacreInteraction {
            accessibleName: parent.label
            function onClicked() {
                root.chooseProfile(parent.profile);
            }
        }
    }
}
