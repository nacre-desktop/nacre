import QtQuick
import QtTest
import Quickshell.Services.UPower
import "fixtures"

TestCase {
    name: "SettingsPowerPolicy"
    when: windowShown
    property var policy: NacrePowerPolicy
    function init() {
        policy.serviceReady = true;
        NacreControlTools.data = {
            powerProfilesSupported: true
        };
        NacreSettingsStorage.documents = {
            power: {
                enabled: true,
                ac: "balanced",
                battery: "power-saver"
            }
        };
        NacreSettingsStorage.present = {};
        PowerProfiles.hasPerformanceProfile = true;
        PowerProfiles.profile = PowerProfile.Balanced;
        UPower.onBattery = false;
    }
    function test_opt_in_and_source_change() {
        UPower.onBattery = true;
        compare(PowerProfiles.profile, PowerProfile.Balanced);
        NacreSettingsStorage.present = {
            power: true
        };
        compare(PowerProfiles.profile, PowerProfile.PowerSaver);
        UPower.onBattery = false;
        compare(PowerProfiles.profile, PowerProfile.Balanced);
        NacreSettingsStorage.documents = {
            power: {
                enabled: true,
                ac: "performance",
                battery: "balanced"
            }
        };
        compare(PowerProfiles.profile, PowerProfile.Performance);
        UPower.onBattery = true;
        compare(PowerProfiles.profile, PowerProfile.Balanced);
    }
    function test_service_and_hardware_limits_do_not_apply() {
        NacreControlTools.data = {
            powerProfilesSupported: false
        };
        NacreSettingsStorage.present = {
            power: true
        };
        UPower.onBattery = true;
        compare(PowerProfiles.profile, PowerProfile.Balanced);
        NacreControlTools.data = {
            powerProfilesSupported: true
        };
        verify(!policy.available);
        policy.serviceReady = true;
        compare(PowerProfiles.profile, PowerProfile.PowerSaver);
        PowerProfiles.hasPerformanceProfile = false;
        NacreSettingsStorage.documents = {
            power: {
                enabled: true,
                ac: "performance",
                battery: "performance"
            }
        };
        compare(PowerProfiles.profile, PowerProfile.PowerSaver);
    }
}
