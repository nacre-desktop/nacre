import QtQuick
import QtTest
import Quickshell
import "fixtures" as Native

TestCase {
    id: test
    name: "NacreCompositorClients"
    when: windowShown

    Component {
        id: window
        QtObject {
            property string address: "0x10"
            property string title: "Editor"
            property bool activated: true
            property var workspace: ({
                    id: 2
                })
            property var handle: null
            property var wayland: null
            property var lastIpcObject: ({
                    class: "Code",
                    title: "Editor",
                    at: [10, 20],
                    size: [800, 600],
                    workspace: {
                        id: 2
                    },
                    pid: 123,
                    monitor: 7,
                    fullscreen: 0,
                    floating: false,
                    focusHistoryID: 0
                })
        }
    }
    function init() {
        Native.Hyprland.toplevels.values = [];
        Native.Hyprland.focusedMonitor = {
            name: "fixture",
            activeWorkspace: {
                id: 2
            }
        };
        Native.Hyprland.monitors = {
            values: [
                {
                    id: 7,
                    name: "fixture"
                }
            ]
        };
        Native.Hyprland.requests = [];
        Native.Hyprland.usingLua = true;
        Native.Hyprland.refreshes = 0;
    }
    function test_stable_identity_reactive_metadata_focus_and_removal() {
        const first = createTemporaryObject(window, test);
        Native.Hyprland.toplevels.values = [first];
        const state = HyprlandBackend;
        compare(state.clients.length, 1);
        const row = state.clients[0];
        compare(state.activeClient, row);
        compare(row.wmClass, "Code");
        compare(row.appId, "Code");
        compare(row.workspaceId, 2);
        compare(row.outputId, "fixture");
        compare(row.captureHandle, null);
        compare(row.fullscreenMode, 0);
        verify(!row.hidden);
        verify(!row.pinned);
        verify(state.capabilities.globalWorkspaces);
        verify(state.capabilities.stableWorkspaceIndices);
        verify(!state.capabilities.perOutputWorkspaces);
        verify(state.capabilities.pinWindow);
        compare(row.width, 800);
        compare(state.activeWsId, 2);
        first.title = "Renamed";
        compare(row.title, "Renamed");
        first.lastIpcObject = Object.assign({}, first.lastIpcObject, {
            fullscreen: 2
        });
        verify(row.fullscreen);
        compare(row.lastIpcObject.fullscreen, 2);
        state.observeFocus("10");
        compare(state.activeClient, row);
        state.observeFocus("");
        compare(state.activeClient, null);
        state.focusKnown = false;
        const second = createTemporaryObject(window, test, {
            address: "0x20",
            activated: false
        });
        Native.Hyprland.toplevels.values = [first, second];
        wait(20);
        compare(state.clients.length, 2);
        compare(state.clients[0], row);
        first.activated = false;
        second.activated = true;
        compare(state.activeClient, state.clients[1]);
        Native.Hyprland.toplevels.values = [second];
        wait(20);
        compare(state.clients.length, 1);
        compare(state.activeClient.address, "0x20");
        second.lastIpcObject = {};
        compare(state.clients[0].width, 0);
        compare(state.clients[0].pid, 0);
        Native.Hyprland.toplevels.values = [];
        wait(20);
        compare(state.activeClient, null);
        compare(state.clients.length, 0);
    }
    function test_lua_workspace_translation_trusted_expressions_and_event_coalescing() {
        const state = HyprlandBackend;
        compare(Native.Hyprland.requests.length, 0);
        verify(state.dispatch("workspace 7"));
        compare(Native.Hyprland.requests[0], "hl.dsp.focus({workspace=7,on_current_monitor=true})");
        verify(!state.dispatch("workspace evil"));
        compare(Native.Hyprland.requests.length, 1);
        verify(state.dispatch('hl.dsp.focus({window="address:0x10"})'));
        compare(Native.Hyprland.requests.length, 2);
        Native.Hyprland.usingLua = false;
        verify(state.dispatch("workspace 3"));
        compare(Native.Hyprland.requests[2], "workspace 3");
        const reads = Native.Hyprland.refreshes;
        for (let i = 0; i < 30; i++)
            Native.Hyprland.rawEvent({
                name: "windowtitlev2",
                data: "ignored"
            });
        wait(80);
        compare(Native.Hyprland.refreshes, reads + 1);
        wait(100);
        compare(Native.Hyprland.refreshes, reads + 1);
    }
    function test_attention_events_clear_on_focus_close_and_removal() {
        const first = createTemporaryObject(window, test);
        const second = createTemporaryObject(window, test, {
            address: "0x20",
            activated: false
        });
        Native.Hyprland.toplevels.values = [first, second];
        const state = HyprlandBackend;
        Native.Hyprland.rawEvent({
            name: "urgent",
            data: "20"
        });
        verify(state.clients[1].urgent);
        verify(state.focusWindow("20"));
        compare(Native.Hyprland.requests[0], 'hl.dsp.focus({window="address:0x20"})');
        verify(!state.focusWindow('20"; forbidden'));
        verify(!state.focusWindow("ffff"));
        Native.Hyprland.rawEvent({
            name: "urgent",
            data: "unknown-address"
        });
        compare(Object.keys(state.urgentAddresses).length, 1);
        state.observeFocus("20");
        verify(!state.clients[1].urgent);
        state.observeFocus("10");
        state.observeUrgency("20");
        verify(state.clients[1].urgent);
        Native.Hyprland.rawEvent({
            name: "closewindow",
            data: "20"
        });
        verify(!state.clients[1].urgent);
        state.observeUrgency("20");
        Native.Hyprland.toplevels.values = [first];
        wait(20);
        compare(Object.keys(state.urgentAddresses).length, 0);
    }
    function test_typed_window_actions_validate_identity_geometry_and_backend_encoding() {
        const first = createTemporaryObject(window, test);
        Native.Hyprland.toplevels.values = [first];
        Native.Hyprland.monitors = {
            values: [
                {
                    name: "fixture"
                }
            ]
        };
        const state = HyprlandBackend;
        compare(state.clients[0].windowId, "0x10");
        compare(state.clients[0].geometry.width, 800);
        verify(!state.moveWindowToWorkspace("unknown", 2));
        verify(!state.moveWindowToWorkspace("10", "2; forbidden"));
        verify(!state.resizeWindow("10", 0, 100));
        verify(!state.placeWindow("10", {
            x: NaN,
            y: 0,
            width: 100,
            height: 100
        }));
        verify(!state.publishPanelRegions(1, [
            {
                x: 0,
                y: 0,
                width: Infinity,
                height: 20
            }
        ]));
        compare(Native.Hyprland.requests.length, 0);
        verify(state.moveWindowToWorkspace("10", 4));
        compare(Native.Hyprland.requests.pop(), 'hl.dsp.window.move({window="address:0x10",workspace="4",follow=false})');
        verify(state.closeWindow("10"));
        compare(Native.Hyprland.requests.pop(), 'hl.dsp.window.close({window="address:0x10"})');
        verify(state.floatWindow("10"));
        compare(Native.Hyprland.requests.pop(), 'hl.dsp.window.float({window="address:0x10",action="toggle"})');
        verify(state.pinWindow("10"));
        compare(Native.Hyprland.requests.pop(), 'hl.dsp.window.pin({window="address:0x10"})');
        verify(state.moveWindow("10", -10, 20));
        compare(Native.Hyprland.requests.pop(), 'hl.dsp.window.move({window="address:0x10",x=-10,y=20,relative=false})');
        verify(state.resizeWindow("10", 700, 500));
        compare(Native.Hyprland.requests.pop(), 'hl.dsp.window.resize({window="address:0x10",x=700,y=500,relative=false})');
        verify(state.placeWindow("10", {
            x: 1,
            y: 2,
            width: 300,
            height: 200
        }));
        compare(Native.Hyprland.requests.slice(-3)[0], 'hl.dsp.window.float({window="address:0x10",action="enable"})');
        Native.Hyprland.requests = [];
        verify(!state.moveWorkspaceToMonitor(2, "unknown"));
        verify(state.moveWorkspaceToMonitor(2, "fixture"));
        compare(Native.Hyprland.requests.pop(), 'hl.dsp.workspace.move({workspace="2",monitor="fixture"})');
        verify(state.logout());
        compare(Native.Hyprland.requests.pop(), "hl.dsp.exit()");
        Native.Hyprland.usingLua = false;
        Native.Hyprland.requests = [];
        verify(state.moveWindowToWorkspace("10", 4, true));
        compare(Native.Hyprland.requests.pop(), "movetoworkspace 4,address:0x10");
        verify(state.resizeWindow("10", 700, 500));
        compare(Native.Hyprland.requests.pop(), "resizewindowpixel exact 700 500,address:0x10");
        verify(state.focusWindow("10"));
        compare(Native.Hyprland.requests.pop(), "focuswindow address:0x10");
        verify(state.closeWindow("10"));
        compare(Native.Hyprland.requests.pop(), "closewindow address:0x10");
        verify(state.floatWindow("10", true));
        compare(Native.Hyprland.requests.pop(), "setfloating address:0x10");
        verify(state.floatWindow("10", false));
        compare(Native.Hyprland.requests.pop(), "settiled address:0x10");
        verify(state.pinWindow("10"));
        compare(Native.Hyprland.requests.pop(), "pin address:0x10");
    }
    function test_metadata_refresh_and_configuration_reload_are_distinct() {
        const state = HyprlandBackend;
        const config = findChild(state, "compositorConfigurationReload");
        verify(!config.running);
        const reads = Native.Hyprland.refreshes;
        state.refreshState();
        compare(Native.Hyprland.refreshes, reads + 1);
        verify(!config.running);
        verify(state.reload());
        verify(config.running);
        verify(state.reloadConfiguration());
        compare(config.command.join(" "), "hyprctl reload");
        compare(Native.Hyprland.refreshes, reads + 1);
    }
    function test_neutral_events_and_serial_geometry_publication() {
        const state = HyprlandBackend;
        const values = [];
        state.event.connect(value => values.push(value));
        let configurations = 0;
        let gesture = "";
        state.configurationReloaded.connect(() => configurations++);
        state.panelGesture.connect(value => gesture = value);
        Native.Hyprland.rawEvent({
            name: "activewindowv2",
            data: "10"
        });
        compare(values[0].type, "window-focus");
        compare(values[0].windowId, "0x10");
        Native.Hyprland.rawEvent({
            name: "configreloaded",
            data: ""
        });
        compare(configurations, 1);
        Native.Hyprland.rawEvent({
            name: "custom",
            data: "nacre-panel-drag,end"
        });
        compare(gesture, "nacre-panel-drag,end");
        verify(state.publishPanelRegions(42, [
            {
                x: 1.5,
                y: 2,
                width: 30,
                height: 40
            }
        ]));
        const writer = findChild(state, "panelGeometryWriter");
        verify(writer.running);
        compare(writer.command[2], "nacre_panel_pid=42;nacre_panel_regions={{x=2,y=2,w=30,h=40}}");
        verify(state.publishPanelRegions(42, []));
        compare(writer.pending, "nacre_panel_pid=42;nacre_panel_regions={}");
        writer.running = false;
        writer.exited(0, 0);
        verify(writer.running);
        compare(writer.command[2], "nacre_panel_pid=42;nacre_panel_regions={}");
    }
    Component {
        id: shortcutWidget
        NacreShortcut {
            property int presses: 0
            property int releases: 0
            name: "fixture-action"
            description: "Fixture shortcut"
            triggerDescription: "Super+A"
            onPressed: presses++
            onReleased: releases++
        }
    }
    function test_shortcut_factory_metadata_events_replacement_and_owner_teardown() {
        Native.ShortcutRegistry.live = [];
        Native.ShortcutRegistry.created = 0;
        Native.ShortcutRegistry.destroyed = 0;
        Native.ShortcutRegistry.duplicates = 0;
        const state = HyprlandBackend;
        Native.Environment.compositor = state;
        const owner = createTemporaryObject(shortcutWidget, test);
        const adapter = owner.registration;
        compare(Native.ShortcutRegistry.live.length, 1);
        compare(Native.ShortcutRegistry.created, 1);
        compare(adapter.nativeShortcut.appid, "nacre_shell");
        compare(adapter.nativeShortcut.name, "fixture-action");
        compare(adapter.nativeShortcut.description, "Fixture shortcut");
        compare(adapter.nativeShortcut.triggerDescription, "Super+A");
        compare(Object.keys(adapter.metadata()).sort().join(","), "appid,description,name,triggerDescription");
        adapter.nativeShortcut.pressed();
        compare(owner.presses, 1);
        verify(owner.held);
        adapter.nativeShortcut.released();
        compare(owner.releases, 1);
        verify(!owner.held);
        owner.name = "updated-action";
        owner.description = "Updated description";
        owner.triggerDescription = "Super+B";
        tryCompare(Native.ShortcutRegistry, "created", 2);
        compare(owner.registration, adapter);
        compare(Native.ShortcutRegistry.destroyed, 1);
        compare(Native.ShortcutRegistry.live.length, 1);
        compare(Native.ShortcutRegistry.duplicates, 0);
        compare(adapter.nativeShortcut.name, "updated-action");
        compare(adapter.nativeShortcut.description, "Updated description");
        compare(adapter.nativeShortcut.triggerDescription, "Super+B");
        adapter.nativeShortcut.pressed();
        compare(owner.presses, 2);
        verify(owner.held);
        owner.appid = "updated-app";
        compare(owner.releases, 2);
        verify(!owner.held);
        tryCompare(Native.ShortcutRegistry, "created", 3);
        compare(adapter.nativeShortcut.appid, "updated-app");
        adapter.nativeShortcut.pressed();
        compare(owner.presses, 3);
        adapter.nativeShortcut.released();
        compare(owner.releases, 3);
        owner.description = "Pending update before closing";
        owner.destroy();
        tryVerify(() => Native.ShortcutRegistry.live.length === 0);
        compare(Native.ShortcutRegistry.created, Native.ShortcutRegistry.destroyed);
        const creationsAfterClose = Native.ShortcutRegistry.created;
        wait(20);
        compare(Native.ShortcutRegistry.created, creationsAfterClose);
        compare(Native.ShortcutRegistry.duplicates, 0);
        Native.Environment.compositor = null;
    }
}
