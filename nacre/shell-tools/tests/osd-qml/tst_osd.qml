import QtQuick
import QtTest
import Quickshell
import Quickshell.Services.UPower
import "fixtures"

TestCase {
    id: test
    name: "SpecificOsdFamily"
    width: 900
    height: 600
    visible: true
    when: windowShown
    function init() {
        NacreAudio.sink = {
            description: "Studio speakers",
            name: "speaker"
        };
        NacreAudio.volume = .4;
        NacreAudio.muted = false;
        NacreAudio.micMuted = false;
        NacreAudio.available = true;
        NacreAudio.micAvailable = true;
        NacreNotifs.dnd = false;
        NacrePowerPolicy.available = true;
        NacreControlTools.data = {
            nightLightSupported: true,
            nightLightEnabled: false
        };
        WallpaperPlayback.locked = false;
        NacrePanelState.hidden = false;
        NacreCompositor.focusedMonitor = {
            name: "test"
        };
        DesktopSettings.data = {
            animations: true
        };
    }
    Component {
        id: events
        NacreOsdEvents {
            screen: ({
                    name: "test"
                })
            visibilities: QtObject {
                property bool osd: false
                property bool session: false
            }
            hovered: false
        }
    }
    Component {
        id: model
        NacreOsdModel {
            output: ({
                    name: "eDP-1"
                })
        }
    }
    Component {
        id: pill
        NacreOsdPill {
            width: implicitWidth
            height: implicitHeight
        }
    }
    function armedEvents() {
        const view = createTemporaryObject(events, test);
        tryCompare(view, "armed", true, 1000);
        verify(!view.shown);
        return view;
    }
    function test_startup_is_quiet_repeats_update_and_extend_deadline() {
        const view = createTemporaryObject(events, test);
        NacreAudio.volume = .5;
        wait(30);
        verify(!view.shown);
        tryCompare(view, "armed", true, 1000);
        NacreAudio.volume = .6;
        tryCompare(view, "shown", true);
        compare(view.channel, "volume");
        wait(70);
        NacreAudio.volume = .7;
        wait(70);
        verify(view.shown);
        tryCompare(view, "shown", false, 300);
    }
    function test_output_change_wins_coincident_volume_notification() {
        const view = armedEvents();
        NacreAudio.sink = {
            description: "Studio headphones",
            name: "bluez.headphones"
        };
        NacreAudio.volume = .8;
        tryCompare(view, "shown", true);
        compare(view.channel, "output");
        const data = createTemporaryObject(model, test, {
            channel: view.channel
        });
        compare(data.detail, "Studio headphones");
        compare(data.glyph, "headphones");
        verify(!data.level);
    }
    function test_mute_levels_and_device_names_are_specific() {
        const data = createTemporaryObject(model, test);
        compare(data.glyph, "volume_down");
        compare(data.detail, "Studio speakers");
        NacreAudio.volume = .8;
        compare(data.glyph, "volume_up");
        NacreAudio.muted = true;
        compare(data.glyph, "volume_off");
        data.channel = "microphone";
        compare(data.detail, "Studio microphone");
        NacreAudio.micMuted = true;
        compare(data.glyph, "mic_off");
        data.channel = "keyboard";
        compare(data.value, .3);
        data.channel = "display";
        Quickshell.screens = [
            {
                name: "eDP-1"
            },
            {
                name: "DP-1"
            }
        ];
        compare(data.detail, "eDP-1");
    }
    function test_toggles_and_supported_hardware_events() {
        const view = armedEvents();
        NacreCapsLock.active = true;
        NacreCapsLock.changed(true);
        tryCompare(view, "channel", "capslock");
        NacreKeyboardLight.adjusted();
        tryCompare(view, "channel", "keyboard");
        NacreBrightness.device.adjusted();
        tryCompare(view, "channel", "display");
        NacreControlTools.data = {
            nightLightSupported: true,
            nightLightEnabled: true
        };
        tryCompare(view, "channel", "night-light");
        NacreNotifs.dnd = true;
        tryCompare(view, "channel", "dnd");
        PowerProfiles.profile = PowerProfile.Performance;
        tryCompare(view, "channel", "power-profile");
    }
    function test_locked_other_monitor_and_open_controls_suppress_feedback() {
        const view = armedEvents();
        WallpaperPlayback.locked = true;
        NacreAudio.volume = .5;
        wait(20);
        verify(!view.shown);
        WallpaperPlayback.locked = false;
        NacreCompositor.focusedMonitor = {
            name: "other"
        };
        NacreAudio.volume = .6;
        wait(20);
        verify(!view.shown);
        NacreCompositor.focusedMonitor = {
            name: "test"
        };
        view.visibilities.osd = true;
        NacreAudio.volume = .7;
        wait(20);
        verify(!view.shown);
        view.visibilities.osd = false;
        NacreAudio.volume = .8;
        tryCompare(view, "shown", true);
        NacreCompositor.focusedMonitor = {
            name: "other"
        };
        compare(view.shown, false);
        NacreCompositor.focusedMonitor = {
            name: "test"
        };
        NacreAudio.volume = .9;
        WallpaperPlayback.locked = true;
        wait(20);
        verify(!view.shown);
    }
    SignalSpy {
        id: capsChanges
        target: CapsReader
        signalName: "changed"
    }
    function test_native_caps_reader_ignores_initial_state_and_reports_changes() {
        const path = decodeURIComponent(Qt.resolvedUrl("leds/input0::capslock/brightness").toString().slice(7));
        Quickshell.files = {
            [path]: "0\n"
        };
        CapsReader.ledFolder = Qt.resolvedUrl("leds");
        tryCompare(CapsReader, "available", true, 1000);
        tryCompare(CapsReader, "ready", true, 1000);
        compare(CapsReader.active, false);
        capsChanges.clear();
        Quickshell.files = {
            [path]: "1\n"
        };
        tryCompare(CapsReader, "active", true, 1000);
        compare(capsChanges.count, 1);
        Quickshell.files = {
            [path]: "invalid\n"
        };
        wait(300);
        verify(CapsReader.active);
        compare(capsChanges.count, 1);
    }
    function test_pill_bounds_and_reduced_motion_updates_in_place() {
        const view = createTemporaryObject(pill, test, {
            title: "Volume",
            detail: "Studio speakers",
            valueLabel: "40%",
            value: .4
        });
        const fill = findChild(view, "feedbackFill"), track = findChild(view, "feedbackTrack");
        verify(findChild(view, "feedbackDetail").width > 0);
        verify(fill.width <= track.width && fill.width > 0);
        DesktopSettings.data = {
            animations: false
        };
        view.value = .8;
        compare(fill.width, track.width * .8);
        view.level = false;
        verify(!track.visible);
        verify(view.Accessible.name.includes("Studio speakers"));
    }
}
