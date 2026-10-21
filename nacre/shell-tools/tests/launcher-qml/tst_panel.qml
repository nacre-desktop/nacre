import QtQuick
import QtTest
import Quickshell
import "fixtures"

TestCase {
    name: "IndependentLauncherPanel"
    width: 1100
    height: 1000
    visible: true
    when: windowShown

    Component {
        id: scene
        Item {
            width: 1100
            height: 1000
            property alias panel: panel
            NacreLauncherPanel {
                id: panel
                width: implicitWidth
                height: implicitHeight
                property PersistentProperties state: PersistentProperties {
                    property bool launcher: false
                    property string launcherMode: "apps"
                    property string launcherQuery: ""
                    property int launcherRequest: 0
                }
                visibilities: state
            }
        }
    }
    function test_panel_focuses_search_on_fresh_open_and_reopen() {
        const view = createTemporaryObject(scene, this), panel = view.panel;
        const page = findChild(panel, "launcherPage");
        panel.state.launcher = true;
        tryCompare(page, "active", true);
        const search = findChild(page.item, "launcherSearch");
        verify(search);
        tryCompare(search, "activeFocus", true, 1200);
        keyClick(Qt.Key_E);
        compare(search.text, "e");

        panel.state.launcher = false;
        tryCompare(panel, "height", 0, 1200);
        search.focus = false;
        search.text = "";
        panel.state.launcher = true;
        tryCompare(search, "activeFocus", true, 1200);
        keyClick(Qt.Key_D);
        compare(search.text, "d");
    }
    function test_slide_caches_fixed_page_only_during_animation() {
        const view = createTemporaryObject(scene, this), panel = view.panel;
        const page = findChild(panel, "launcherPage");
        panel.state.launcher = true;
        wait(80);
        verify(panel.opening);
        compare(page.height, 300);
        verify(page.layer.enabled);
        wait(350);
        verify(!panel.opening);
        verify(!page.layer.enabled);
        compare(page.height, 300);
    }

    function test_body_and_search_share_one_translation_through_focus_and_close() {
        const view = createTemporaryObject(scene, this), panel = view.panel;
        const page = findChild(panel, "launcherPage");
        panel.state.launcher = true;
        tryCompare(page, "status", Loader.Ready);
        const search = findChild(page.item, "launcherSearch");
        let previousBody = null, previousSearch = null;
        for (let frame = 0; frame < 24; frame++) {
            const bodyY = panel.mapToItem(view, 0, 0).y;
            const searchY = search.mapToItem(view, 0, 0).y;
            compare(panel.height, 300);
            compare(page.height, 300);
            fuzzyCompare(searchY - bodyY, 240, .001);
            if (previousBody !== null)
                fuzzyCompare(searchY - previousSearch, bodyY - previousBody, .001);
            previousBody = bodyY;
            previousSearch = searchY;
            if (frame === 4)
                panel.focusExplicit();
            wait(16);
        }
        verify(search.activeFocus);
        fuzzyCompare(panel.slideOffset, 0, .001);
        compare(panel.reveal, 1);
        panel.state.launcher = false;
        wait(80);
        compare(panel.height, 300);
        verify(panel.reveal > 0 && panel.reveal < 1);
        tryCompare(panel, "height", 0);
    }

    function test_lazy_open_reversal_and_unload() {
        const view = createTemporaryObject(scene, this), panel = view.panel;
        const page = findChild(panel, "launcherPage");
        verify(!page.active);
        page.keepWarm = false;
        page.releaseDelay = 100;
        panel.state.launcher = true;
        wait(400);
        compare(panel.height, 300);
        verify(page.item);
        compare(page.item.kind, "apps");
        panel.state.launcher = false;
        wait(80);
        verify(panel.presentedHeight > 0 && panel.presentedHeight < 300);
        compare(panel.height, 300);
        compare(page.height, 300);
        verify(page.item);
        panel.state.launcher = true;
        wait(400);
        compare(panel.height, 300);
        panel.state.launcher = false;
        wait(400);
        compare(panel.height, 0);
        tryCompare(page, "active", false, 500);
        verify(!page.item);
    }
    function test_apps_stay_warm_but_gallery_returns_to_idle_policy() {
        const view = createTemporaryObject(scene, this), panel = view.panel;
        const page = findChild(panel, "launcherPage");
        page.releaseDelay = 80;
        panel.state.launcher = true;
        wait(400);
        const apps = page.item;
        panel.state.launcher = false;
        wait(450);
        compare(page.item, apps);
        verify(page.keepWarm);
        panel.state.launcher = true;
        compare(page.item, apps);
        panel.state.launcherMode = "wallpaper";
        verify(!page.keepWarm);
        panel.state.launcher = false;
        tryCompare(page, "active", false, 1000);
        verify(!page.item);
    }
    function test_modes_and_full_gallery_contracts() {
        const view = createTemporaryObject(scene, this), panel = view.panel;
        panel.state.launcher = true;
        for (const mode of ["palette", "legacy", "overview", "clipboard", "keys", "apps"]) {
            panel.state.launcherMode = mode;
            wait(20);
            const item = findChild(panel, "launcherPage").item;
            if (mode === "legacy")
                verify(findChild(item, "legacyLauncherSearch"));
            else
                compare(item.kind, mode);
        }
        panel.state.launcherMode = "wallpaper";
        wait(20);
        compare(panel.galleryCount, 3);
        compare(panel.galleryIndex, 0);
        panel.galleryStep(1);
        compare(panel.galleryIndex, 1);
        NacreWallpapers.preferences = {
            layout: "spotlight"
        };
        compare(panel.fullScreenGallery, true);
        compare(panel.contentHeight, view.height);
        compare(panel.implicitWidth, view.width);
        NacreWallpapers.preferences = {
            layout: "carousel"
        };
        compare(panel.fullScreenGallery, false);
    }
    Component {
        id: searchPanel
        NacreSearchPanel {
            width: 680
            height: implicitHeight
            visibilities: PersistentProperties {
                property bool launcher: true
                property string launcherQuery: ""
                property int launcherRequest: 0
            }
        }
    }
    function test_fresh_fallback_search_actions_and_keyboard() {
        const view = createTemporaryObject(searchPanel, this);
        wait(30);
        const field = findChild(view, "legacyLauncherSearch");
        field.text = "editor";
        compare(view.entries.length, 1);
        compare(view.entries[0].id, "editor");
        view.activate(0);
        compare(NacreApps.last, "editor");
        view.visibilities.launcher = true;
        field.text = ">";
        compare(view.entries.length, 1);
        compare(view.entries[0].action, "settings");
        field.forceActiveFocus();
        keyClick(Qt.Key_Return);
        compare(DesktopActions.last, "settings");
        compare(view.visibilities.launcher, false);
        view.visibilities.launcher = true;
        field.text = "nothing matches";
        compare(view.entries.length, 0);
        keyClick(Qt.Key_Return);
        compare(view.visibilities.launcher, true);
        keyClick(Qt.Key_Escape);
        compare(view.visibilities.launcher, false);
    }
}
