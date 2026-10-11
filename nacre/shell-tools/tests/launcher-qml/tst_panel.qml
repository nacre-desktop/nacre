import QtQuick
import QtTest
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
                property QtObject state: QtObject {
                    property bool launcher: false
                    property string launcherMode: "apps"
                    property string launcherQuery: ""
                    property int launcherRequest: 0
                }
                visibilities: state
            }
        }
    }
    function test_lazy_open_reversal_and_unload() {
        const view = createTemporaryObject(scene, this), panel = view.panel;
        const page = findChild(panel, "launcherPage");
        verify(!page.active);
        panel.state.launcher = true;
        wait(400);
        compare(panel.height, 300);
        verify(page.item);
        compare(page.item.kind, "apps");
        panel.state.launcher = false;
        wait(80);
        verify(panel.height > 0 && panel.height < 300);
        compare(page.height, 300);
        verify(page.item);
        panel.state.launcher = true;
        wait(400);
        compare(panel.height, 300);
        panel.state.launcher = false;
        wait(400);
        compare(panel.height, 0);
        verify(!page.active);
        verify(!page.item);
    }
    function test_modes_and_full_gallery_contracts() {
        const view = createTemporaryObject(scene, this), panel = view.panel;
        panel.state.launcher = true;
        for (const mode of ["palette", "legacy", "overview", "clipboard", "keys", "apps"]) {
            panel.state.launcherMode = mode;
            wait(20);
            compare(findChild(panel, "launcherPage").item.kind, mode);
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
            visibilities: QtObject {
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
