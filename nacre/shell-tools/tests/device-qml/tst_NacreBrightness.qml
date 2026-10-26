import QtQuick
import QtTest
import Quickshell
import "fixtures"

TestCase {
    id: test
    name: "NacreScreenLightOwner"
    when: windowShown
    function init() {
        Quickshell.screens = [
            {
                name: "eDP-1"
            }
        ];
    }

    function test_mixed_scale_screens_without_backlight_are_readonly() {
        Quickshell.screens = [
            {
                name: "DP-1",
                scale: 1
            },
            {
                name: "HDMI-A-1",
                scale: 1.5
            },
            {
                name: "eDP-1",
                scale: 2
            }
        ];
        const state = NacreBrightness;
        compare(state.monitors.length, 3);
        for (const monitor of state.monitors) {
            verify(!monitor.available);
            verify(!monitor.setBrightness(.5));
            compare(monitor.writer.starts, 0);
        }
        Quickshell.screens = [
            {
                name: "eDP-1"
            }
        ];
        state.reconcile();
        compare(state.monitors.length, 1);
    }
    function test_screen_mapping_no_mount_write_and_closed_timer() {
        const state = NacreBrightness;
        compare(state.monitors.length, 1);
        verify(!state.monitors[0].available);
        state.hardware = {
            backlight: {
                kind: "backlight",
                device: "fixture",
                maximum: 100,
                current: 50
            },
            keyboard: null,
            ddc: []
        };
        verify(state.monitors[0].available);
        compare(state.monitors[0].brightness, .5);
        verify(state.applyDaemonLevel("eDP-1", .65));
        compare(state.monitors[0].brightness, .65);
        verify(!state.applyDaemonLevel("missing", .4));
        let externalSteps = 0;
        state.monitors[0].externalAdjusted.connect(() => externalSteps++);
        state.monitors[0].parseCurrent("60");
        compare(state.monitors[0].brightness, .6);
        compare(externalSteps, 1);
        compare(state.monitors[0].writer.starts, 0);
        verify(!findChild(state, "screenLightTimer").running);
        NacrePanelState.screens = {
            test: {
                osd: true
            }
        };
        verify(findChild(state, "screenLightTimer").running);
        NacrePanelState.screens = {};
        const external = {
            name: "DP-1"
        };
        Quickshell.screens = [external];
        state.reconcile();
        verify(!state.monitors[0].available);
        Quickshell.screens = [];
        state.reconcile();
        compare(state.monitors.length, 0);
        Quickshell.screens = [
            {
                name: "eDP-1"
            }
        ];
    }
}
