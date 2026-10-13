pragma ComponentBehavior: Bound
import QtQuick
import Quickshell
import qs.widgets
import qs.services
import qs.modules.extras as Extras

Item {
    id: root
    required property PersistentProperties visibilities
    property string screenName: ""
    function prepareLift() {
        edgeOpening.complete();
    }
    readonly property bool opening: visibilities.launcher && edgeOpening.running
    property bool restoreImmediately: false
    function restoreAttached() {
        edgeOpening.complete();
        presentedHeight = Qt.binding(() => visibilities.launcher ? contentHeight : 0);
    }
    property bool handoffHidden: false
    opacity: handoffHidden ? 0 : 1
    onEnabledChanged: if (enabled)
        handoffHidden = false
    property bool detached: false
    property bool closeAfterLaunch: false
    property var menuContext: NacreMenu
    signal closeRequested
    signal uiChanged
    readonly property string mode: visibilities.launcherMode || "apps"
    readonly property bool fullScreenGallery: mode === "wallpaper" && (NacreWallpapers.preferences.layout || "carousel") !== "carousel"
    readonly property real viewportWidth: Math.max(0, parent?.width ?? 0)
    readonly property real viewportHeight: Math.max(0, parent?.height ?? 0)
    readonly property real contentHeight: detached ? viewportHeight : fullScreenGallery ? Math.max(0, viewportHeight) : (page.item?.implicitHeight ?? 300)
    readonly property int galleryCount: mode === "wallpaper" ? page.item?.count ?? 0 : 0
    readonly property int galleryIndex: mode === "wallpaper" ? page.item?.currentIndex ?? -1 : -1
    property real presentedHeight: visibilities.launcher ? contentHeight : 0
    implicitHeight: presentedHeight
    implicitWidth: detached ? viewportWidth : fullScreenGallery ? Math.max(0, viewportWidth) : page.item?.implicitWidth ?? 800
    visible: height > 0
    clip: true

    readonly property bool canDetach: !page.item?.picker && !page.item?.form && !page.item?.confirmation
    function focusExplicit() {
        if (page.item?.focusExplicit)
            page.item.focusExplicit();
        else
            page.item?.forceActiveFocus();
    }
    function captureState() {
        return Object.assign({
            mode: mode
        }, page.item?.captureState ? page.item.captureState() : {});
    }
    function restoreState(data) {
        if (page.item?.restoreState)
            page.item.restoreState(data);
    }
    function galleryStep(delta) {
        if (mode === "wallpaper" && page.item)
            page.item.move(delta);
    }
    NacreIdleLoader {
        id: page
        y: 0
        objectName: "launcherPage"
        wanted: root.visibilities.launcher || root.presentedHeight > 0
        keepWarm: root.mode === "apps"
        width: root.width
        height: Math.max(0, root.contentHeight - y)
        focus: true
        sourceComponent: ({
                apps: applications,
                wallpaper: wallpapers,
                palette: palette,
                legacy: legacy,
                overview: overview,
                clipboard: clipboard,
                keys: shortcuts
            })[root.mode] || applications
    }
    Component {
        id: applications
        NacreAppBrowser {
            viewportWidth: root.viewportWidth
            viewportHeight: root.viewportHeight
            visibilities: root.visibilities
            detached: root.detached
            menu: root.menuContext
            closeAfterLaunch: root.closeAfterLaunch
            onCloseRequested: root.closeRequested()
            onQueryChanged: root.uiChanged()
            onRouteChanged: root.uiChanged()
        }
    }
    Component {
        id: wallpapers
        NacreWallpaperPicker {
            detached: root.detached
            viewportWidth: root.viewportWidth
            viewportHeight: root.viewportHeight
            visibilities: root.visibilities
        }
    }
    Component {
        id: palette
        Extras.Palette {
            detached: root.detached
            visibilities: root.visibilities
        }
    }
    Component {
        id: legacy
        NacreSearchPanel {
            detached: root.detached
            visibilities: root.visibilities
        }
    }
    Component {
        id: overview
        Extras.Overview {
            detached: root.detached
            visibilities: root.visibilities
        }
    }
    Component {
        id: clipboard
        Extras.Clipboard {
            detached: root.detached
            visibilities: root.visibilities
        }
    }
    Component {
        id: shortcuts
        Extras.Keybindings {
            detached: root.detached
            visibilities: root.visibilities
        }
    }

    Behavior on presentedHeight {
        enabled: !root.restoreImmediately || !root.visibilities.launcher
        NumberAnimation {
            id: edgeOpening
            duration: NacreTokens.motionEnabled ? 300 : 0
            easing.type: Easing.OutCubic
        }
    }
    Keys.onEscapePressed: if (!root.detached)
        root.visibilities.launcher = false
}
