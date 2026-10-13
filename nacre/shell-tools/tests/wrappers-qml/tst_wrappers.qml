import QtQuick
import QtTest
import "fixtures"

TestCase {
    id: test
    name: "NacreDesktopWrappers"
    width: 800
    height: 600
    visible: true
    when: windowShown
    function init() {
        DesktopSettings.data = {
            animations: true
        };
        NacreAudio.available = false;
        NacreAudio.volume = 0;
        NacreAudio.writes = [];
        NacreKeyboardLight.writes = [];
        NacreBrightness.device.writes = [];
        WallpaperPlayback.locked = false;
        WallpaperPlayback.sleeping = false;
        WallpaperPlayback.paused = false;
        WallpaperPlayback.batteryPaused = false;
        NacreCompositor.focusedMonitor = {
            name: "test"
        };
        NacreCompositor.windows = [];
        NacrePanelState.screens = ({});
        NacreWallpapers.displayDynamic = false;
        NacreWallpapers.displayAnimated = false;
        NacrePresentation.pending = ({});
        NacrePresentation.active = ({});
        NacrePresentation.activations = [];
        AppLaunch.calls = [];
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
        id: session
        NacreSessionPanel {
            visibilities: QtObject {
                property bool session: false
            }
        }
    }
    Component {
        id: scene
        NacreWallpaperScene {
            screenName: "test"
            width: 240
            height: 180
            source: ""
        }
    }
    property var sessionTarget: null
    Keys.onEscapePressed: event => {
        if (sessionTarget) {
            sessionTarget.visibilities.session = false;
            event.accepted = true;
        }
    }
    function test_session_focus_and_escape_reach_parent_controller() {
        const panel = createTemporaryObject(session, test);
        sessionTarget = panel;
        panel.visibilities.session = true;
        tryCompare(panel, "focus", true, 400);
        keyClick(Qt.Key_Escape);
        verify(!panel.visibilities.session);
        compare(AppLaunch.calls.length, 0);
        sessionTarget = null;
    }
    function test_ready_image_waits_for_late_matching_presentation() {
        const view = createTemporaryObject(scene, test);
        const path = decodeURIComponent(Qt.resolvedUrl("first.png").toString().slice(7));
        view.source = view.fileUrl(path);
        tryCompare(view.current, "status", Image.Ready, 1000);
        compare(view.displayedPath, "");
        NacrePresentation.pending = {
            poster: path
        };
        NacrePresentation.revision++;
        tryCompare(view, "displayedPath", path, 1000);
        compare(NacrePresentation.active.poster, path);
    }
    function test_osd_ready_baseline_focus_hover_and_deadline() {
        const view = createTemporaryObject(events, test);
        wait(1);
        NacreAudio.available = true;
        NacreAudio.volume = .4;
        wait(1);
        verify(!view.shown);
        NacreAudio.volume = .5;
        verify(view.shown);
        view.hovered = true;
        wait(120);
        verify(view.shown);
        view.hovered = false;
        tryCompare(view, "shown", false, 200);
        NacreCompositor.focusedMonitor = {
            name: "other"
        };
        NacreBrightness.device.adjusted();
        verify(!view.shown);
        NacreCompositor.focusedMonitor = {
            name: "test"
        };
        NacreKeyboardLight.adjusted();
        verify(view.shown);
        compare(view.channel, "keyboard");
        view.visibilities.osd = true;
        compare(view.shown, false);
        NacreAudio.volume = .6;
        verify(!view.shown);
        view.visibilities.osd = false;
        view.shown = false;
        WallpaperPlayback.locked = true;
        NacreKeyboardLight.adjusted();
        verify(!view.shown);
    }
    function test_session_display_no_commands_allowlist_and_closing() {
        const panel = createTemporaryObject(session, test);
        panel.visibilities.session = true;
        tryVerify(() => panel.width > 0, 300);
        compare(AppLaunch.calls.length, 0);
        const controls = panel.children[0].item;
        verify(controls);
        verify(!controls.activate("unknown"));
        compare(AppLaunch.calls.length, 0);
        verify(controls.activate("lock"));
        compare(AppLaunch.calls[0].join("|"), "loginctl|lock-session");
        verify(!panel.visibilities.session);
        verify(!controls.activate("restart"));
        wait(240);
        compare(panel.width, 0);
    }
    function record(view, file) {
        const path = decodeURIComponent(Qt.resolvedUrl(file).toString().slice(7));
        NacrePresentation.pending = {
            poster: path
        };
        view.source = view.fileUrl(path);
        return path;
    }
    function test_poster_latest_ready_pair_opaque_underlayer_and_literal_percent() {
        const view = createTemporaryObject(scene, test);
        const first = record(view, "first.png");
        tryCompare(view, "displayedPath", first, 1000);
        compare(view.current.opacity, 1);
        const old = view.current;
        const second = record(view, "second.png");
        tryCompare(view, "displayedPath", second, 1000);
        verify(view.transitioning);
        compare(old.opacity, 1);
        compare(NacrePresentation.active.poster, second);
        const third = record(view, "percent % picture.png");
        tryCompare(view, "displayedPath", third, 1500);
        wait(350);
        verify(!view.transitioning);
        compare(view.current.opacity, 1);
        compare(NacrePresentation.active.poster, third);
        compare(NacrePresentation.activations.length, 3);
        const slots = view.children.filter(child => typeof child.requestPath === "string");
        compare(slots.filter(slot => slot.source.toString().length > 0).length, 2);
        verify(slots.some(slot => slot.requestPath === second && slot.status === Image.Ready));
        verify(!slots.some(slot => slot.requestPath === first));
        record(view, "second.png");
        tryCompare(view, "displayedPath", second, 1000);
        wait(350);
        compare(view.current.opacity, 1);
        DesktopSettings.data = {
            animations: false
        };
        wait(10);
        compare(slots.filter(slot => slot.source.toString().length > 0).length, 2);
        view.releaseDelay = 80;
        view.finish();
        tryVerify(() => slots.filter(slot => slot.source.toString().length > 0).length === 1, 1000);
        compare(view.current.requestPath, second);
        compare(view.current.status, Image.Ready);
        record(view, "first.png");
        tryCompare(view, "displayedPath", first, 1000);
    }
    function test_motion_policy_changes_do_not_publish_or_write() {
        const view = createTemporaryObject(scene, test);
        verify(view.motionAllowed);
        WallpaperPlayback.sleeping = true;
        verify(!view.motionAllowed);
        WallpaperPlayback.sleeping = false;
        WallpaperPlayback.locked = true;
        verify(!view.motionAllowed);
        WallpaperPlayback.locked = false;
        NacrePanelState.screens = {
            test: {
                launcher: true,
                launcherMode: "wallpaper"
            }
        };
        verify(view.motionAllowed);
        WallpaperPlayback.paused = true;
        verify(!view.motionAllowed);
        WallpaperPlayback.paused = false;
        WallpaperPlayback.batteryPaused = true;
        verify(!view.motionAllowed);
        WallpaperPlayback.batteryPaused = false;
        NacrePanelState.screens = ({});
        NacreCompositor.windows = [
            {
                workspace: {
                    id: 1
                },
                floating: false,
                fullscreen: false
            }
        ];
        verify(!view.motionAllowed);
        compare(NacrePresentation.activations.length, 0);
        compare(AppLaunch.calls.length, 0);
    }
}
