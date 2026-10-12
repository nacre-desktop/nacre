import QtQuick
import QtTest
import "fixtures"

TestCase {
    id: test
    name: "NacreShellState"
    when: windowShown
    Component {
        id: bridge
        NacreShellIpc {}
    }
    Component {
        id: keys
        NacreShellShortcuts {}
    }
    Component {
        id: flags
        QtObject {
            property bool previewOnly: false
            property bool hoverDocked: false
            property bool osd: false
            property bool session: false
            property bool launcher: false
            property bool left: false
            property bool leftPinned: false
            property bool dashboard: false
            property bool dashboardPinned: false
            property int dashboardTab: 0
            property string launcherQuery: ""
            property string launcherMode: "apps"
            property string launcherRoute: "home"
            property int launcherRequest: 0
            property string edgeMenu: ""
            property string controlSection: "home"
        }
    }
    property var one: null
    property var two: null
    function test_docked_hover_resets_on_deliberate_open() {
        NacrePanelState.openEdge("dashboard", "one");
        NacrePanelState.dockAsHover(one);
        verify(one.dashboard);
        verify(one.hoverDocked);
        compare(one.edgeMenu, "");
        NacrePanelState.openEdge("dashboard", "one");
        verify(!one.hoverDocked);
        compare(one.edgeMenu, "dashboard");
        NacrePanelState.dockAsHover(one);
        NacrePanelState.hoverEdge("left", "one");
        verify(!one.hoverDocked);
        NacrePanelState.openMode("wallpaper", "", false, "one", true);
        NacrePanelState.dockAsHover(one);
        verify(one.launcher);
        verify(one.hoverDocked);
        NacrePanelState.openMenu();
        verify(!one.hoverDocked);
    }
    function init() {
        NacreSettingsApp.close();
        NacreSettingsApp.created = false;
        one = createTemporaryObject(flags, test);
        two = createTemporaryObject(flags, test);
        NacrePanelState.screens = {
            one: one,
            two: two
        };
        NacrePanelState.panels = {
            one: {
                popouts: {
                    hasCurrent: false,
                    pinned: false,
                    currentName: "",
                    currentCenter: 0,
                    width: 200,
                    height: 100,
                    headerHovered: false
                },
                launcher: {
                    galleryCount: 20,
                    galleryIndex: 2
                },
                notifications: {
                    suppressed: false
                },
                leftDrawer: {
                    section: "chat",
                    width: 400,
                    height: 700
                }
            },
            two: {
                popouts: {
                    hasCurrent: false,
                    pinned: false,
                    currentName: "",
                    currentCenter: 0,
                    width: 200,
                    height: 100,
                    headerHovered: false
                },
                launcher: {
                    galleryCount: 10,
                    galleryIndex: 0
                },
                notifications: {
                    suppressed: false
                },
                leftDrawer: {
                    section: "chat",
                    width: 400,
                    height: 700
                }
            }
        };
        NacreCompositor.focusedMonitor = {
            name: "one"
        };
        NacreCompositor.calls = [];
        NacreHoverIntent.dismissed = [];
        DisplayRecovery.restores = [];
        NacrePanelState.hidden = false;
    }
    function cleanup() {
        NacrePanelState.screens = ({});
        NacrePanelState.panels = ({});
    }
    function test_router_modes_preview_toggle_and_per_output_fallback() {
        verify(NacrePanelState.openMode("wallpaper", "", true));
        verify(one.launcher && one.previewOnly);
        compare(two.launcher, false);
        const requests = one.launcherRequest;
        NacrePanelState.openMode("wallpaper", "", true);
        verify(!one.launcher);
        compare(one.launcherRequest, requests + 1);
        verify(!NacrePanelState.openMode("invalid"));
        verify(!NacrePanelState.popout("calendar", NaN, "one"));
        NacreCompositor.focusedMonitor = null;
        compare(NacrePanelState.getForActive(), one);
        NacreCompositor.focusedMonitor = {
            name: "two"
        };
        NacrePanelState.openMode("apps", "query", false);
        verify(two.launcher);
        compare(two.launcherQuery, "query");
    }
    function test_settings_lifecycle_focus_and_minimized_activity() {
        const window = Qt.createQmlObject('import QtQuick; QtObject {property bool minimized:false;property int activations:0;function activate(){activations++}}', test);
        NacreSettingsApp.window = window;
        NacreSettingsApp.open("sound");
        verify(NacreSettingsApp.visible && NacreSettingsApp.created);
        verify(NacrePanelState.settingsVisible);
        compare(NacrePanelState.settingsPage, "sound");
        NacreSettingsApp.open("network");
        compare(NacreSettingsApp.window, window);
        compare(window.activations, 2);
        window.minimized = true;
        compare(NacrePanelState.settingsVisible, false);
        NacreSettingsApp.open();
        compare(window.minimized, false);
        compare(NacrePanelState.settingsPage, "network");
        NacreCompositor.windows = [
            {
                pid: 0,
                title: "Nacre Settings",
                wmClass: "org.quickshell",
                windowId: "0xabcd"
            }
        ];
        NacreSettingsApp.focusExisting();
        NacreSettingsApp.minimize();
        const job = findChild(NacreSettingsApp, "settingsMinimizeProcess");
        verify(job.running);
        compare(job.command.slice(-4), ["hide", "--window", "0xabcd", "--quiet"]);
        NacreSettingsApp.open();
        verify(NacreSettingsApp.focusPending);
        job.running = false;
        job.exited(0, 0);
        NacreSettingsApp.focusExisting();
        verify(NacreCompositor.calls[NacreCompositor.calls.length - 1].includes("0xabcd"));
        verify(!NacreSettingsApp.focusPending);
        NacreSettingsApp.close();
        verify(!NacrePanelState.settingsVisible && !NacreSettingsApp.visible);
        verify(NacreSettingsApp.created);
        NacreSettingsApp.window = null;
        NacreCompositor.windows = [];
        window.destroy();
    }
    function test_settings_left_manual_pin_and_competing_transients() {
        one.left = true;
        one.leftPinned = true;
        one.launcher = true;
        NacrePanelState.panels.one.popouts.pinned = true;
        NacrePanelState.panels.one.popouts.hasCurrent = true;
        verify(NacrePanelState.openDeviceSettings("audio"));
        compare(NacrePanelState.getForActive().controlSection, "audio");
        verify(NacrePanelState.getForActive().osd);
        verify(one.left && one.leftPinned && !one.launcher && !one.dashboard);
        verify(NacrePanelState.openSettings("sound"));
        verify(NacreSettingsApp.visible && NacreSettingsApp.created);
        compare(NacrePanelState.settingsPage, "sound");
        verify(!one.dashboard && !one.dashboardPinned && !one.launcher);
        verify(one.left && one.leftPinned);
        verify(!NacrePanelState.panels.one.popouts.hasCurrent && !NacrePanelState.panels.one.popouts.pinned);
        NacrePanelState.toggleLeft();
        verify(!one.left && !one.leftPinned);
        NacrePanelState.toggleLeft();
        verify(one.left && !one.leftPinned);
        compare(one.edgeMenu, "left");
        NacrePanelState.toggleSession();
        verify(one.session && !one.dashboard && !one.launcher);
        NacrePanelState.close();
        verify(!one.session && !one.left && !one.leftPinned);
        compare(one.edgeMenu, "");
        compare(NacreHoverIntent.dismissed[0], "one");
    }

    function test_bridge_readonly_state_gallery_bounds_and_restore() {
        const view = createTemporaryObject(bridge, test);
        const state = view.stateData();
        compare(state.active, 2);
        compare(state.workspaces, 2);
        verify(!state.launcher);
        compare(view.galleryData()[0].count, 20);
        compare(view.popupData()[0].open, false);
        view.workspace(7);
        compare(NacreCompositor.calls[0], "workspace 7");
        view.workspace(-1);
        compare(NacreCompositor.calls.length, 1);
        view.tab(4);
        verify(!NacreSettingsApp.visible);
        verify(one.dashboard && !one.dashboardPinned);
        compare(one.dashboardTab, 4);
        view.tab(99);
        compare(one.dashboardTab, 4);
        verify(!view.restore("{"));
        verify(!view.restore("[]"));
        verify(!view.restore('{"version":2}'));
        verify(view.restore('{"version":2,"screens":{}}'));
        compare(DisplayRecovery.restores.length, 1);
        verify(view.restore('{"left":true,"leftPinned":true,"leftSection":"brain"}'));
        verify(one.left && one.leftPinned);
        compare(NacrePanelState.panels.one.leftDrawer.section, "brain");
    }
    function test_shortcut_interruption_and_passive_escape_all_outputs() {
        const shortcut = createTemporaryObject(keys, test);
        shortcut.launcherPress();
        shortcut.launcherInterrupt();
        shortcut.launcherRelease();
        verify(!one.launcher);
        shortcut.launcherPress();
        shortcut.launcherRelease();
        verify(one.launcher);
        one.launcher = false;
        one.dashboard = true;
        one.dashboardPinned = true;
        two.dashboard = true;
        one.session = true;
        shortcut.dismissPassive();
        verify(one.dashboard);
        verify(!two.dashboard);
        one.session = false;
        shortcut.dismissPassive();
        verify(one.dashboard);
        shortcut.toggle("dashboard");
        verify(!one.dashboard);
        verify(!shortcut.toggle("unknown"));
    }
}
