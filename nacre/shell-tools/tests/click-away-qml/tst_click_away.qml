import QtQuick
import QtTest
import "fixtures"

TestCase {
    id: test

    function test_edge_guard_rearms_only_after_leaving_and_respects_drag_fullscreen() {
        const screen = {
            "name": "test",
            "width": 500,
            "height": 600
        };
        NacreHoverIntent.observe(screen, 250, 1);
        NacreHoverIntent.dismiss(screen);
        verify(!NacreHoverIntent.canOpen("dashboard", screen, Qt.NoButton));
        NacreHoverIntent.observe(screen, 250, 2);
        verify(!NacreHoverIntent.canOpen("dashboard", screen, Qt.NoButton));
        NacreHoverIntent.observe(screen, 250, 40);
        verify(NacreHoverIntent.canOpen("dashboard", screen, Qt.NoButton));
        verify(!NacreHoverIntent.canOpen("dashboard", screen, Qt.LeftButton));
        NacreHyprland.activeClient = {
            "lastIpcObject": {
                "fullscreen": 2
            }
        };
        verify(!NacreHoverIntent.canOpen("dashboard", screen, Qt.NoButton));
        NacreHyprland.activeClient = {
            "lastIpcObject": {
                "fullscreen": 1
            }
        };
        verify(NacreHoverIntent.canOpen("dashboard", screen, Qt.NoButton));
        NacreHyprland.activeClient = null;
    }

    function test_modal_header_hover_loss_keeps_geometric_dismiss_guard() {
        const screen = {
            name: "geometric",
            width: 1920,
            height: 1200
        };
        NacreHoverIntent.recordPopupRegion(screen, 1550, 8, 210, 34);
        NacreHoverIntent.observe(screen, 1700, 25);
        NacreHoverIntent.popupHovered[screen.name] = false;
        NacreHoverIntent.dismiss(screen);
        verify(!NacreHoverIntent.canOpen("popouts", screen, Qt.NoButton));
        NacreHoverIntent.rearm("popouts", screen);
        verify(NacreHoverIntent.canOpen("popouts", screen, Qt.NoButton));
        NacreHoverIntent.observe(screen, 900, 1000);
        NacreHoverIntent.dismiss(screen);
        verify(NacreHoverIntent.canOpen("popouts", screen, Qt.NoButton));
    }
    function test_clicks_inside_keep_open_and_outside_dismiss() {
        const view = createTemporaryObject(scene, test);
        verify(view);
        mouseClick(view, 150, 400);
        verify(view.visibilities.launcher);
        mouseClick(view, 50, 100);
        verify(!view.visibilities.launcher);
    }

    function test_click_opened_sidebar_stays_open_without_pinning_and_dismisses_outside() {
        const view = createTemporaryObject(scene, test);
        view.visibilities.launcher = false;
        view.visibilities.edgeMenu = "left";
        view.visibilities.left = true;
        mouseMove(view, 400, 500);
        wait(160);
        verify(view.visibilities.left);
        verify(!view.visibilities.leftPinned);
        mouseClick(view, 150, 200);
        verify(view.visibilities.left);
        mouseClick(view, 450, 500);
        verify(!view.visibilities.left);
    }

    function test_click_opened_controls_stay_open_and_dismiss_outside() {
        const view = createTemporaryObject(scene, test);
        view.visibilities.launcher = false;
        view.visibilities.edgeMenu = "osd";
        view.visibilities.osd = true;
        mouseMove(view, 20, 500);
        wait(160);
        verify(view.visibilities.osd);
        mouseClick(view, 350, 200);
        verify(view.visibilities.osd);
        mouseClick(view, 20, 500);
        verify(!view.visibilities.osd);
    }

    function test_right_click_outside_also_dismisses() {
        const view = createTemporaryObject(scene, test);
        verify(view);
        mouseClick(view, 450, 400, Qt.RightButton);
        verify(!view.visibilities.launcher);
    }

    function test_closing_releases_the_region_before_visual_geometry_finishes() {
        const view = createTemporaryObject(scene, test);
        verify(view.launcherRect.width > 0);
        view.visibilities.launcher = false;
        compare(view.panels.launcher.width, 200);
        compare(view.launcherRect.width, 0);
        verify(!view.modal);
        verify(!view.regions.some(r => r.x === 100 && r.y === 350 && r.width === 200));
        view.visibilities.left = true;
        verify(view.leftRect.width > 0);
        view.visibilities.left = false;
        compare(view.leftRect.width, 0);
    }
    function test_pin_uses_nonmodal_input_and_hidden_clears_regions() {
        const view = createTemporaryObject(scene, test);
        view.visibilities.launcher = false;
        view.visibilities.edgeMenu = "left";
        view.visibilities.left = true;
        verify(view.modal);
        view.visibilities.leftPinned = true;
        verify(!view.modal);
        view.hidden = true;
        compare(view.regions.length, 0);
        compare(view.leftRect.width, 0);
    }
    function test_regions_follow_parent_offset_and_size_updates() {
        const view = createTemporaryObject(scene, test);
        view.panels.x = 12;
        view.panels.y = 50;
        compare(view.launcherRect.x, 112);
        compare(view.launcherRect.y, 400);
        view.panels.launcher.width = 220;
        compare(view.launcherRect.width, 220);
        view.panels.launcher.x = 60;
        compare(view.launcherRect.x, 72);
    }
    function test_passive_hover_closes_but_pinned_and_explicit_panels_remain() {
        const view = createTemporaryObject(scene, test);
        view.visibilities.launcher = false;
        view.visibilities.left = true;
        mouseMove(view, 150, 200);
        tryCompare(view, "leftHovered", true);
        mouseMove(view, 450, 550);
        wait(180);
        verify(!view.visibilities.left);
        view.visibilities.left = true;
        view.visibilities.leftPinned = true;
        mouseMove(view, 150, 200);
        mouseMove(view, 450, 550);
        wait(180);
        verify(view.visibilities.left);
    }
    function test_pointer_handler_does_not_steal_child_clicks() {
        const view = createTemporaryObject(scene, test);
        const child = Qt.createQmlObject('import QtQuick; MouseArea {width:80;height:40;property int count:0;onClicked:count++}', view);
        child.x = 100;
        child.y = 360;
        mouseMove(child, 20, 20);
        mouseClick(child, 20, 20);
        compare(child.count, 1);
        verify(view.visibilities.launcher);
        child.destroy();
    }

    name: "ClickAway"
    width: 500
    height: 600
    visible: true
    when: windowShown

    Component {
        id: scene

        NacrePanelInput {
            width: 500
            height: 600

            screen: QtObject {
                property string name: "test"
                property int width: 500
                property int height: 600
            }

            visibilities: QtObject {
                property string edgeMenu: ""
                property bool dashboardPinned: false
                property bool launcher: true
                property bool left: false
                property bool leftPinned: false
                property bool dashboard: false
                property bool osd: false
                property bool session: false
                property bool previewOnly: false
            }

            panels: Item {
                property alias launcher: panel
                property alias leftDrawer: leftPanel
                property alias osd: rightPanel
                property alias dashboard: dashboardPanel
                property alias session: rightPanel
                property alias popouts: popup
                property alias notifications: notices
                Item {
                    id: popup
                    property bool hasCurrent: false
                    property bool headerHovered: false
                    property bool pinned: false
                }
                Item {
                    id: notices
                    visible: false
                }

                Item {
                    id: leftPanel

                    x: 10
                    y: 100
                    width: 200
                    height: 300
                }

                Item {
                    id: rightPanel

                    x: 300
                    y: 100
                    width: 190
                    height: 300
                }

                Item {
                    id: dashboardPanel

                    x: 100
                    y: 0
                    width: 300
                    height: 200
                }

                Item {
                    id: panel

                    x: 100
                    y: 350
                    width: 200
                    height: 200
                }
            }
        }
    }
}
