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
        id: pearlView
        NacreWorkspacePearl {}
    }
    Component {
        id: symbolIcon
        NacreWorkspaceAppIcon {}
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
        NacreBar.workspaceIcons = [];
        NacreBar.workspaceCount = 5;
        DesktopSettings.data = {
            animations: true
        };
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
        verify(view.activate(5));
        compare(NacreHyprland.requests[0], "workspace 5");
        verify(!view.activate(8));
        verify(!view.activate(NaN));
        compare(NacreHyprland.requests.length, 1);
    }
    function test_numbered_pearls_use_readable_ink_for_dark_and_pale_accents() {
        const pearl = createTemporaryObject(pearlView, test, {
            workspaceId: 12,
            occupied: true,
            accent: "#000000"
        });
        const number = findChild(pearl, "workspaceNumber");
        compare(number.text, "12");
        compare(number.color, Qt.color("#f5f2ee"));
        pearl.accent = "#ffffff";
        compare(number.color, Qt.color("#303036"));
        for (const accent of ["#b5a3d5", "#62789b", "#797979", "#555555"]) {
            pearl.accent = accent;
            const ink = pearl.luminance(number.color);
            const surface = pearl.numberLuminance;
            verify((Math.max(ink, surface) + .05) / (Math.min(ink, surface) + .05) >= 4.5);
        }
    }
    function test_symbol_classification_and_pearls_only_toggle() {
        for (const sample of [
            {
                name: "kitty",
                glyph: "terminal"
            },
            {
                name: "firefox",
                glyph: "language"
            },
            {
                name: "discord",
                glyph: "forum"
            },
            {
                name: "nacre-ai-dashboard",
                glyph: "neurology"
            },
            {
                name: "nacre-brain",
                glyph: "psychology"
            },
            {
                name: "nacre-settings",
                glyph: "tune"
            },
            {
                name: "editor",
                glyph: "code"
            }
        ]) {
            const icon = createTemporaryObject(symbolIcon, test, {
                client: {
                    wmClass: sample.name
                }
            });
            compare(icon.symbol, sample.glyph);
        }
        const view = createTemporaryObject(row, test);
        NacreHyprland.clients = [
            {
                workspace: {
                    id: 2
                },
                wmClass: "kitty"
            }
        ];
        tryVerify(() => view.activeCell?.windowCount === 1);
        verify(view.showWindowHints);
        DesktopSettings.data = {
            workspaceWindowHints: false
        };
        tryVerify(() => !view.showWindowHints);
        compare(view.activeCell.shownWindows.length, 0);
        compare(view.activeCell.overflow, 0);
        tryCompare(view.activeCell, "width", 32);
        verify(!findChild(view.activeCell, "workspaceApps").visible);
        verify(findChild(view, "activeWorkspacePearl").visible);
    }
    function test_pearls_keep_extra_windows_accessible_and_follow_active_workspace() {
        const view = createTemporaryObject(row, test);
        compare(view.workspaceIds.join(","), "1,2,3,4,5");
        const selected = findChild(view, "nacreWorkspace2");
        tryVerify(() => view.activeCell === selected);
        verify(findChild(view, "activeWorkspacePearl").visible);
        compare(findChild(view, "activeWorkspacePearl").width, 22);
        const empty = findChild(findChild(view, "nacreWorkspace1"), "workspacePearl");
        compare(empty.width, 18);
        compare(findChild(empty, "workspaceNumber").text, "1");
        compare(findChild(findChild(view, "activeWorkspacePearl"), "workspaceNumber").text, "2");
        NacreHyprland.clients = [
            {
                workspace: {
                    id: 8
                }
            },
            {
                workspace: {
                    id: 8
                }
            },
            {
                workspace: {
                    id: -99
                }
            }
        ];
        tryCompare(view, "workspaceIds", [1, 2, 3, 4, 5, 8]);
        tryVerify(() => !!findChild(view, "nacreWorkspace8"));
        compare(findChild(view, "nacreWorkspace8").windowCount, 2);
        verify(view.activate(8));
        NacreHyprland.clients = [];
        NacreHyprland.activeWsId = 8;
        tryVerify(() => !!findChild(view, "nacreWorkspace8"));
        verify(findChild(view, "nacreWorkspace8").selected);
        tryCompare(findChild(findChild(view, "activeWorkspacePearl"), "workspaceNumber"), "text", "8");
        NacreHyprland.activeWsId = 1;
        tryCompare(view, "workspaceIds", [1, 2, 3, 4, 5]);
        NacreBar.workspaceIcons = ["language"];
        verify(view.customIcons);
        verify(!findChild(findChild(view, "nacreWorkspace1"), "workspacePearl").visible);
    }
    function test_capsule_icons_overflow_click_and_scroll_are_bounded() {
        const view = createTemporaryObject(row, test);
        NacreHyprland.clients = Array.from({
            length: 5
        }, (_, index) => ({
                    workspace: {
                        id: 2
                    },
                    wmClass: "editor",
                    title: "Window " + index
                }));
        tryVerify(() => !!view.activeCell);
        compare(view.activeCell.shownWindows.length, 3);
        compare(view.activeCell.overflow, 2);
        verify(findChild(view.activeCell, "workspaceOverflow").visible);
        const stable = findChild(view, "nacreWorkspace1");
        NacreHyprland.activeWsId = 4;
        tryVerify(() => view.activeCell?.workspaceId === 4);
        compare(findChild(view, "nacreWorkspace1"), stable);
        compare(view.activeCell.shownWindows.length, 0);
        compare(findChild(view, "nacreWorkspace2").shownWindows.length, 0);
        verify(!findChild(findChild(view, "nacreWorkspace2"), "workspaceApps").visible);
        verify(view.scroll(-60));
        compare(NacreHyprland.requests.length, 0);
        verify(view.scroll(-60));
        compare(NacreHyprland.requests[0], "workspace 5");
        NacreHyprland.activeWsId = 5;
        verify(!view.step(1));
        verify(!view.step(0));
        compare(NacreHyprland.requests.length, 1);
        NacreHyprland.activeWsId = 2;
        wait(250);
        const cell = findChild(view, "nacreWorkspace2");
        mouseClick(cell, 16, 15);
        compare(NacreHyprland.requests[1], "workspace 2");
        view.availableWidth = 200;
        tryVerify(() => view.iconLimit === 0);
        compare(view.activeCell.overflow, 5);
    }
    function test_attention_reduced_motion_and_icon_focus() {
        const view = createTemporaryObject(row, test);
        NacreHyprland.clients = [
            {
                workspace: {
                    id: 3
                },
                urgent: true,
                wmClass: "editor",
                title: "Attention",
                address: "0x20"
            },
            {
                workspace: {
                    id: 2
                },
                wmClass: "editor",
                title: "Editor",
                address: "0x10"
            }
        ];
        const pearl = findChild(findChild(view, "nacreWorkspace3"), "workspacePearl");
        const pulse = findChild(pearl, "workspaceAttentionPulse");
        tryVerify(() => pulse.running);
        verify(findChild(pearl, "workspaceAttentionRing").visible);
        DesktopSettings.data = {
            reduceMotion: true
        };
        tryCompare(pulse, "running", false);
        verify(findChild(pearl, "workspaceAttentionRing").visible);
        NacreHyprland.activeWsId = 3;
        tryVerify(() => view.activeCell?.workspaceId === 3);
        const capsule = findChild(view, "activeWorkspaceCapsule");
        compare(capsule.x, view.activeCell.x);
        NacreHyprland.activeWsId = 2;
        tryVerify(() => view.activeCell?.workspaceId === 2);
        const active = findChild(view, "nacreWorkspace2");
        tryCompare(active, "width", 54);
        wait(250);
        compare(findChild(view, "nacreWorkspace3").x, active.x + active.width + 2);
        mouseClick(active, 41, 15);
        compare(NacreHyprland.requests[0], "focus 0x10");
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
