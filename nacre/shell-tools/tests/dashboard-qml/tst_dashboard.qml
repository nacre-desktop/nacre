import QtQuick
import QtTest
import "fixtures"

TestCase {
    id: test
    name: "NacreDashboardAssembly"
    width: 1200
    height: 1000
    visible: true
    when: windowShown
    Component {
        id: panel
        NacreDashboardPanel {
            visibilities: QtObject {
                property bool dashboard: true
                property bool dashboardPinned: false
                property int dashboardTab: 0
            }
        }
    }
    function init() {
        DesktopSettings.data = {
            animations: true
        };
        NacrePanelState.hidden = false;
        NacreDashboardState.tabs = ["today", "weather", "media", "system", "focus"];
    }
    function test_pages_and_immediate_navigation_pinning() {
        const view = createTemporaryObject(panel, test);
        wait(300);
        const loader = findChild(view, "dashboardPage");
        const navigation = findChild(view, "dashboardNavigation");
        const marker = findChild(view, "dashboardSelection");
        compare(loader.item.pageName, "NacreTodayPage");
        compare(loader.item.shouldUpdate, true);
        for (const [index, name] of ["NacreTodayPage", "NacreWeatherPage", "NacreMediaPage", "NacreSystemPage", "NacreFocusPage"].entries()) {
            mouseClick(findChild(view, "dashboardTab" + index), 35, 15);
            compare(view.visibilities.dashboardTab, index);
            compare(navigation.currentIndex, index);
            tryCompare(marker, "x", index * navigation.width / 5 + (navigation.width / 5 - marker.width) / 2);
            compare(view.visibilities.dashboardPinned, false);
            compare(loader.item.pageName, name);
        }
        mouseClick(findChild(view, "dashboardTab0"), 35, 15);
        compare(view.visibilities.dashboardPinned, false);
    }
    function test_keyboard_and_wheel_follow_visible_tab_order() {
        const view = createTemporaryObject(panel, test);
        wait(300);
        NacreDashboardState.tabs = ["focus", "today", "system"];
        view.forceActiveFocus();
        keyClick(Qt.Key_1);
        compare(view.currentIndex, 4);
        const nav = findChild(view, "dashboardNavigation");
        mouseWheel(nav, 100, 15, 0, -120);
        compare(view.currentIndex, 0);
        wait(180);
        mouseWheel(nav, 100, 15, 0, -120);
        compare(view.currentIndex, 3);
        keyClick(Qt.Key_2);
        compare(view.currentIndex, 0);
    }
    function test_closing_keeps_internal_geometry_then_unloads_and_reverses() {
        const view = createTemporaryObject(panel, test);
        wait(300);
        const loader = findChild(view, "dashboardPage");
        const content = loader.height;
        view.visibilities.dashboard = false;
        wait(90);
        verify(loader.active);
        compare(loader.height, content);
        compare(loader.item.shouldUpdate, false);
        verify(view.height > 0);
        view.visibilities.dashboard = true;
        wait(300);
        verify(loader.item.shouldUpdate);
        view.visibilities.dashboard = false;
        tryCompare(view, "height", 0, 500);
        compare(loader.active, false);
        compare(loader.item, null);
    }
    function test_focus_and_hidden_tabs_are_dashboard_pages() {
        const view = createTemporaryObject(panel, test);
        view.visibilities.dashboardTab = 4;
        wait(30);
        compare(view.currentIndex, 4);
        compare(findChild(view, "dashboardPage").item.pageName, "NacreFocusPage");
        NacreDashboardState.tabs = ["system", "today"];
        wait(30);
        compare(view.currentIndex, 3);
        view.select(1);
        compare(view.currentIndex, 3);
        view.select(0);
        compare(view.currentIndex, 0);
    }
    function test_occluded_attached_and_minimized_pearl_release_pages() {
        const view = createTemporaryObject(panel, test);
        wait(30);
        const loader = findChild(view, "dashboardPage");
        verify(loader.item);
        NacrePanelState.hidden = true;
        compare(loader.active, false);
        compare(loader.item, null);
        NacrePanelState.hidden = false;
        tryVerify(() => !!loader.item);
        view.detached = true;
        view.hostActive = false;
        compare(loader.active, false);
        compare(loader.item, null);
        view.hostActive = true;
        tryVerify(() => !!loader.item);
    }
    function test_viewport_and_reduced_motion() {
        DesktopSettings.data = {
            animations: false
        };
        const host = Qt.createQmlObject('import QtQuick; Item {width: 600;height: 500}', test);
        const view = createTemporaryObject(panel, host);
        wait(30);
        verify(view.width <= host.width - 48);
        verify(view.height <= host.height - 64);
        view.visibilities.dashboard = false;
        wait(10);
        compare(view.height, 0);
        compare(findChild(view, "dashboardPage").active, false);
        host.destroy();
    }
}
