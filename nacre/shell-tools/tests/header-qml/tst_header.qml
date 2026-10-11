import QtQuick
import QtTest
import "fixtures"

TestCase {
    id: test
    width: 1920
    height: 400
    visible: true
    when: windowShown
    name: "NacreHeader"
    Component {
        id: row
        NacreWorkspaceRow {}
    }
    Component {
        id: header
        NacreHeader {
            width: 1920
            height: 50
            screen: ({
                    name: "test",
                    width: 1920,
                    height: 1200
                })
        }
    }
    function init() {
        NacreHyprland.requests = [];
        NacrePanelState.calls = [];
        AppLaunch.calls = [];
        NacreHyprland.activeWsId = 2;
        NacreHyprland.clients = [];
    }
    function test_workspace_readonly_state_and_explicit_activation() {
        const view = createTemporaryObject(row, test);
        compare(NacreHyprland.requests.length, 0);
        verify(findChild(view, "nacreWorkspace2").selected);
        NacreHyprland.clients = [
            {
                workspace: {
                    id: 3
                }
            }
        ];
        verify(findChild(view, "nacreWorkspace3").occupied);
        verify(view.activate(7));
        compare(NacreHyprland.requests[0], "workspace 7");
        verify(!view.activate(8));
        verify(!view.activate(NaN));
        compare(NacreHyprland.requests.length, 1);
    }
    function test_header_display_no_commands_and_status_geometry() {
        const view = createTemporaryObject(header, test);
        wait(1);
        compare(NacrePanelState.calls.length, 0);
        compare(AppLaunch.calls.length, 0);
        compare(view.width, 1920);
        compare(view.height, 50);
        compare(NacreHoverIntent.owners.test, view);
    }
    function test_header_popup_contract_and_manual_device_routes() {
        const panel = {
            popouts: {
                hasCurrent: false,
                currentName: "",
                currentCenter: 0,
                pinned: false,
                headerHovered: false
            },
            input: {
                modal: false
            }
        };
        NacrePanelState.panels = {
            test: panel
        };
        const view = createTemporaryObject(header, test);
        const status = findChild(view, "nacreHeaderStatusMouse");
        const region = findChild(view, "nacreHeaderStatus");
        verify(status && region);
        NacreHoverIntent.rearm("popouts", view.screen);
        view.showPopup("battery", status, true);
        compare(NacrePanelState.calls[0], "battery");
        verify(panel.popouts.headerHovered);
        verify(NacreHoverIntent.popupRegions.test.width > 0);
        NacrePanelState.screens.test.session = true;
        const calls = NacrePanelState.calls.length;
        view.showPopup("notifications", status, true);
        compare(NacrePanelState.calls.length, calls);
        NacrePanelState.screens.test.session = false;
        NacrePanelState.panels = ({});
    }
    function test_open_dashboard_keeps_its_full_header_width_without_widening_initial_target() {
        NacrePanelState.screens = {
            test: {
                dashboard: false,
                session: false,
                launcher: false
            }
        };
        NacrePanelState.panels = {
            test: {
                dashboard: {
                    width: 1000
                },
                input: {
                    modal: false,
                    settleHover: function () {}
                }
            }
        };
        const view = createTemporaryObject(header, test);
        wait(20);
        const target = findChild(view, "nacreHeaderCenterBand");
        compare(target.width, 208);
        mouseMove(view, 600, 25);
        verify(!NacreHoverIntent.headers.test);
        NacrePanelState.screens = {
            test: {
                dashboard: true,
                session: false,
                launcher: false
            }
        };
        wait(20);
        compare(target.width, 1000);
        mouseMove(view, 600, 25);
        tryCompare(view, "openHeaderHovered", true);
        verify(NacreHoverIntent.headers.test);
        mouseMove(view, 1300, 25);
        verify(NacreHoverIntent.headers.test);
        mouseMove(view, 1550, 25);
        tryCompare(view, "openHeaderHovered", false);
        verify(!NacreHoverIntent.headers.test);
        NacrePanelState.screens = {
            test: {
                dashboard: false,
                session: false,
                launcher: false
            }
        };
        compare(target.width, 208);
        NacrePanelState.panels = ({});
    }
    function test_header_registration_follows_replaced_screen_and_safe_teardown() {
        const view = createTemporaryObject(header, test);
        compare(NacreHoverIntent.owners.test, view);
        view.screen = {
            name: "replacement",
            width: 1920,
            height: 1200
        };
        compare(view.registeredName, "replacement");
        verify(!NacreHoverIntent.owners.test);
        compare(NacreHoverIntent.owners.replacement, view);
    }
    function test_hover_ownership_invalid_points_and_geometric_popup_block() {
        const screen = {
            name: "ownership",
            width: 1920,
            height: 1200
        };
        const older = Qt.createQmlObject('import QtQuick; QtObject {}', test), newer = Qt.createQmlObject('import QtQuick; QtObject {}', test);
        NacreHoverIntent.register(screen.name, older);
        NacreHoverIntent.observe(screen, 1700, 20);
        NacreHoverIntent.recordPopupRegion(screen, 1600, 8, 200, 34);
        NacreHoverIntent.dismiss(screen);
        verify(!NacreHoverIntent.canOpen("popouts", screen, Qt.NoButton));
        NacreHoverIntent.observe(screen, NaN, 25);
        compare(NacreHoverIntent.positions[screen.name].x, 1700);
        NacreHoverIntent.register(screen.name, newer);
        NacreHoverIntent.release(screen.name, older);
        compare(NacreHoverIntent.owners[screen.name], newer);
        NacreHoverIntent.release(screen.name, newer);
        verify(!NacreHoverIntent.positions[screen.name]);
        verify(!NacreHoverIntent.blocked[screen.name]);
        older.destroy();
        newer.destroy();
    }
}
