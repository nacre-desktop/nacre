import QtQuick
import QtTest
import "registry.js" as Registry
import "layout.js" as Layout

TestCase {
    name: "PanelOwnership"
    function test_replacement_then_old_teardown_keeps_new_output_and_other_screen() {
        const service = createTemporaryObject(store, this);
        const oldState = createTemporaryObject(value, this);
        const oldHost = createTemporaryObject(value, this);
        const newState = createTemporaryObject(value, this);
        const newHost = createTemporaryObject(value, this);
        const otherState = createTemporaryObject(value, this);
        const otherHost = createTemporaryObject(value, this);
        Registry.register(service, "first", oldState, oldHost);
        Registry.register(service, "second", otherState, otherHost);
        const oldMap = service.screens;
        Registry.register(service, "first", newState, newHost);
        verify(oldMap !== service.screens);
        compare(service.screens.first, newState);
        Registry.release(service, "first", oldState, oldHost);
        compare(service.screens.first, newState);
        compare(service.panels.first, newHost);
        compare(service.screens.second, otherState);
        Registry.release(service, "first", newState, newHost);
        verify(!("first" in service.screens));
        verify(!("first" in service.panels));
        compare(service.screens.second, otherState);
        compare(service.panels.second, otherHost);
    }
    function test_partial_registration_cleanup_does_not_remove_unrelated_host() {
        const service = createTemporaryObject(store, this);
        const state = createTemporaryObject(value, this);
        const host = createTemporaryObject(value, this);
        Registry.register(service, "first", state, host);
        Registry.release(service, "first", state, null);
        verify(!("first" in service.screens));
        compare(service.panels.first, host);
        Registry.release(null, "first", state, host);
    }
    function test_attachment_positions_follow_viewport_resize_and_intrinsic_sizes() {
        compare(Layout.attached("left", 1000, 800, 200, 400), {
            x: 0,
            y: 200
        });
        compare(Layout.attached("right", 1000, 800, 200, 400, 50), {
            x: 750,
            y: 200
        });
        compare(Layout.attached("bottom", 1000, 800, 600, 300), {
            x: 200,
            y: 500
        });
        compare(Layout.attached("top", 1000, 800, 600, 300), {
            x: 200,
            y: 0
        });
        compare(Layout.attached("bottom", 600, 500, 600, 300), {
            x: 0,
            y: 200
        });
        compare(Layout.attached("left", 100, 80, 200, 400), {
            x: 0,
            y: 0
        });
    }
    function test_popup_centers_and_docks_without_crossing_viewport() {
        const center = Layout.popout(10, 1000, 510, 200, 200, 25);
        compare(center, {
            x: 400,
            y: 0,
            joinsRight: false
        });
        const right = Layout.popout(10, 1000, 980, 200, 200, 25);
        compare(right, {
            x: 800,
            y: 0,
            joinsRight: true
        });
        const opening = Layout.popout(10, 1000, 980, 20, 200, 25);
        compare(opening.joinsRight, true);
        compare(opening.x + 20, 1000);
        const oversized = Layout.popout(0, 100, 50, 200, 200, 25);
        compare(oversized.x, 0);
    }
    Component {
        id: store
        QtObject {
            property var screens: ({})
            property var panels: ({})
        }
    }
    Component {
        id: value
        QtObject {}
    }
}
