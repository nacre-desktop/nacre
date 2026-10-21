import QtQuick
import Quickshell
import Quickshell.Services.UPower
import qs.services

QtObject {
    id: root
    property var output: null
    property string channel: "volume"
    property int screenCount: Quickshell.screens.length
    readonly property var monitor: NacreBrightness.getMonitorForScreen(output)
    readonly property bool level: ["volume", "microphone", "display", "keyboard"].includes(channel)
    readonly property real value: channel === "display" ? monitor?.brightness || 0 : channel === "keyboard" ? NacreKeyboardLight.brightness : channel === "microphone" ? NacreAudio.micVolume : NacreAudio.volume
    readonly property bool muted: channel === "volume" ? NacreAudio.muted : channel === "microphone" ? NacreAudio.micMuted : false
    readonly property bool enabledState: channel === "capslock" ? NacreCapsLock.active : channel === "dnd" ? NacreNotifs.dnd : channel === "night-light" ? NacreControlTools.data.nightLightEnabled === true : !muted
    readonly property string title: {
        switch (channel) {
        case "output":
            return NacreI18n.tr("osd.feedback.output", "Audio output");
        case "display":
            return NacreI18n.tr("osd.level-notice.text.display-brightness", "Display brightness");
        case "keyboard":
            return NacreI18n.tr("osd.level-notice.text.keyboard-brightness", "Keyboard brightness");
        case "microphone":
            return NacreI18n.tr("osd.level-notice.text.microphone", "Microphone");
        case "capslock":
            return NacreI18n.tr("osd.feedback.caps-lock", "Caps Lock");
        case "power-profile":
            return NacreI18n.tr("osd.feedback.power-mode", "Power mode");
        case "night-light":
            return NacreI18n.tr("controlcenter.overview.night-light", "Night light");
        case "dnd":
            return NacreI18n.tr("controlcenter.overview.do-not-disturb", "Do Not Disturb");
        default:
            return NacreI18n.tr("osd.level-notice.text.volume", "Volume");
        }
    }
    readonly property string detail: {
        switch (channel) {
        case "volume":
        case "output":
            return NacreAudio.sink?.description || NacreI18n.tr("osd.feedback.default-output", "Default output");
        case "microphone":
            return NacreAudio.source?.description || NacreI18n.tr("osd.feedback.default-input", "Default input");
        case "display":
            return screenCount > 1 ? output?.name || "" : "";
        case "keyboard":
            return NacreI18n.tr("osd.feedback.keyboard-backlight", "Keyboard backlight");
        case "power-profile":
            return PowerProfiles.profile === PowerProfile.PowerSaver ? NacreI18n.tr("popouts.battery-popup.label.power-saver", "Power saver") : PowerProfiles.profile === PowerProfile.Performance ? NacreI18n.tr("popouts.battery-popup.label.performance", "Performance") : NacreI18n.tr("popouts.battery-popup.label.balanced", "Balanced");
        default:
            return enabledState ? NacreI18n.tr("osd.feedback.on", "On") : NacreI18n.tr("osd.feedback.off", "Off");
        }
    }
    readonly property string glyph: {
        switch (channel) {
        case "volume":
            return muted ? "volume_off" : value <= .01 ? "volume_mute" : value < .5 ? "volume_down" : "volume_up";
        case "output":
            {
                const description = ((NacreAudio.sink?.description || "") + " " + (NacreAudio.sink?.name || "")).toLowerCase();
                return /headphone|headset|earbud/.test(description) ? "headphones" : /bluez|bluetooth/.test(description) ? "bluetooth_audio" : "speaker";
            }
        case "microphone":
            return muted ? "mic_off" : "mic";
        case "display":
            return value < .5 ? "brightness_low" : "brightness_high";
        case "keyboard":
            return "keyboard";
        case "capslock":
            return "keyboard_capslock";
        case "power-profile":
            return PowerProfiles.profile === PowerProfile.PowerSaver ? "eco" : PowerProfiles.profile === PowerProfile.Performance ? "bolt" : "balance";
        case "night-light":
            return "bedtime";
        case "dnd":
            return enabledState ? "notifications_off" : "notifications_active";
        default:
            return "volume_up";
        }
    }
}
