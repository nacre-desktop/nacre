import QtQuick
import QtTest
import "fixtures" as Native

TestCase {
    id: test
    name: "NacreCompositorClients"
    when: windowShown
    Component {
        id: service
        NacreHyprland {}
    }
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
        Native.Hyprland.requests = [];
        Native.Hyprland.usingLua = true;
        Native.Hyprland.refreshes = 0;
    }
    function test_stable_identity_reactive_metadata_focus_and_removal() {
        const first = createTemporaryObject(window, test);
        Native.Hyprland.toplevels.values = [first];
        const state = createTemporaryObject(service, test);
        compare(state.clients.length, 1);
        const row = state.clients[0];
        compare(state.activeClient, row);
        compare(row.wmClass, "Code");
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
        const state = createTemporaryObject(service, test);
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
}
