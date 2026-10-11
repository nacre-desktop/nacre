pragma ComponentBehavior: Bound
import QtQuick
import Quickshell
import qs.widgets
import qs.services
import qs.modules.extras as Extras

Item {
    id: root
    required property PersistentProperties visibilities
    readonly property string mode: visibilities.launcherMode || "apps"
    readonly property bool fullScreenGallery: mode === "wallpaper" && (NacreWallpapers.preferences.layout || "carousel") !== "carousel"
    readonly property real viewportWidth: Math.max(0, parent?.width ?? 0)
    readonly property real viewportHeight: Math.max(0, parent?.height ?? 0)
    readonly property real contentHeight: fullScreenGallery ? Math.max(0, viewportHeight) : page.item?.implicitHeight ?? 300
    readonly property int galleryCount: mode === "wallpaper" ? page.item?.count ?? 0 : 0
    readonly property int galleryIndex: mode === "wallpaper" ? page.item?.currentIndex ?? -1 : -1
    property real presentedHeight: visibilities.launcher ? contentHeight : 0
    implicitHeight: presentedHeight
    implicitWidth: fullScreenGallery ? Math.max(0, viewportWidth) : page.item?.implicitWidth ?? 800
    visible: height > 0
    clip: true

    function galleryStep(delta) {
        if (mode === "wallpaper" && page.item)
            page.item.move(delta);
    }
    Loader {
        id: page
        objectName: "launcherPage"
        active: root.visibilities.launcher || root.presentedHeight > 0
        width: root.width
        height: root.contentHeight
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
        }
    }
    Component {
        id: wallpapers
        NacreWallpaperPicker {
            viewportWidth: root.viewportWidth
            viewportHeight: root.viewportHeight
            visibilities: root.visibilities
        }
    }
    Component {
        id: palette
        Extras.Palette {
            visibilities: root.visibilities
        }
    }
    Component {
        id: legacy
        NacreSearchPanel {
            visibilities: root.visibilities
        }
    }
    Component {
        id: overview
        Extras.Overview {
            visibilities: root.visibilities
        }
    }
    Component {
        id: clipboard
        Extras.Clipboard {
            visibilities: root.visibilities
        }
    }
    Component {
        id: shortcuts
        Extras.Keybindings {
            visibilities: root.visibilities
        }
    }

    Behavior on presentedHeight {
        NumberAnimation {
            duration: NacreTokens.motionEnabled ? 300 : 0
            easing.type: Easing.OutCubic
        }
    }
    Keys.onEscapePressed: root.visibilities.launcher = false
}
