import QtQuick
import QtTest
import Quickshell.Services.UPower
import "fixtures"

TestCase {
    name: "SettingsPowerPolicy"
    when: windowShown
    property var policy: NacrePowerPolicy
    function init() {
        policy.profileWriter.running = false;
        policy.applied = "";
        policy.error = "";
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
    function acknowledge(expected) {
        verify(policy.profileWriter.running);
        compare(policy.profileWriter.command.join("|"), "/usr/bin/python3|/usr/bin/powerprofilesctl|set|" + expected);
        policy.profileWriter.finish(0);
        compare(policy.applied, expected);
    }
    function test_opt_in_and_source_change() {
        UPower.onBattery = true;
        compare(PowerProfiles.profile, PowerProfile.Balanced);
        NacreSettingsStorage.present = {
            power: true
        };
        acknowledge("power-saver");
        UPower.onBattery = false;
        acknowledge("balanced");
        NacreSettingsStorage.documents = {
            power: {
                enabled: true,
                ac: "performance",
                battery: "balanced"
            }
        };
        acknowledge("performance");
        UPower.onBattery = true;
        acknowledge("balanced");
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
        acknowledge("power-saver");
        PowerProfiles.hasPerformanceProfile = false;
        NacreSettingsStorage.documents = {
            power: {
                enabled: true,
                ac: "performance",
                battery: "performance"
            }
        };
        verify(!policy.profileWriter.running);
    }
    function test_failed_write_is_visible_and_new_source_is_queued() {
        NacreSettingsStorage.present = {
            power: true
        };
        compare(policy.profileWriter.requested, "balanced");
        UPower.onBattery = true;
        policy.profileWriter.finish(1);
        tryCompare(policy.profileWriter, "requested", "power-saver");
        verify(policy.profileWriter.running);
        policy.profileWriter.finish(1);
        verify(policy.error.length > 0);
        compare(policy.applied, "");
    }
}
