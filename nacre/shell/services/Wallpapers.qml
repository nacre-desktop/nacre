pragma Singleton
import Quickshell

Singleton {
    readonly property var currentNamePath: NacreWallpapers.currentNamePath
    readonly property var path: NacreWallpapers.path
    readonly property var list: NacreWallpapers.list
    readonly property var preferences: NacreWallpapers.preferences
    readonly property var palettePresets: NacreWallpapers.palettePresets
    readonly property var paletteOptions: NacreWallpapers.paletteOptions
    readonly property var selectedAccent: NacreWallpapers.selectedAccent
    readonly property var rotationRemaining: NacreWallpapers.rotationRemaining
    readonly property var rotationPool: NacreWallpapers.rotationPool
    readonly property var rotationReady: NacreWallpapers.rotationReady
    readonly property var rotationAllowed: NacreWallpapers.rotationAllowed
    readonly property var rotationIntervalMs: NacreWallpapers.rotationIntervalMs
    readonly property var rotationAnchorMs: NacreWallpapers.rotationAnchorMs
    readonly property var rotationDueMs: NacreWallpapers.rotationDueMs
    readonly property var rotationStatus: NacreWallpapers.rotationStatus
    readonly property var themeBusy: NacreWallpapers.themeBusy
    readonly property var media: NacreWallpapers.media
    readonly property var lastImage: NacreWallpapers.lastImage
    readonly property var actualCurrent: NacreWallpapers.actualCurrent
    readonly property var selectedPath: NacreWallpapers.selectedPath
    readonly property var queuedPath: NacreWallpapers.queuedPath
    readonly property var error: NacreWallpapers.error
    readonly property var loading: NacreWallpapers.loading
    readonly property var current: NacreWallpapers.current
    readonly property var currentEntry: NacreWallpapers.currentEntry
    readonly property var poster: NacreWallpapers.poster
    readonly property var preview: NacreWallpapers.preview
    readonly property var thumbnail: NacreWallpapers.thumbnail
    readonly property var pendingPoster: NacreWallpapers.pendingPoster
    readonly property var displayEntry: NacreWallpapers.displayEntry
    readonly property var displayPath: NacreWallpapers.displayPath
    readonly property var displayPreview: NacreWallpapers.displayPreview
    readonly property var displayDynamic: NacreWallpapers.displayDynamic
    readonly property var displayAnimated: NacreWallpapers.displayAnimated
    readonly property var dynamic: NacreWallpapers.dynamic
    readonly property var animated: NacreWallpapers.animated
    function fuzzyQuery(value) {
        return NacreWallpapers.fuzzyQuery(value);
    }
    function refresh() {
        return NacreWallpapers.refresh();
    }
    function prepareCache() {
        return NacreWallpapers.prepareCache();
    }
    function browse(value) {
        return NacreWallpapers.browse(value);
    }
    function commitSelection() {
        return NacreWallpapers.commitSelection();
    }
    function setWallpaper(value) {
        return NacreWallpapers.setWallpaper(value);
    }
    function preference(value) {
        return NacreWallpapers.preference(value);
    }
    function addFiles(value) {
        return NacreWallpapers.addFiles(value);
    }
    function pickFiles() {
        return NacreWallpapers.pickFiles();
    }
    function advanceRotation(value) {
        return NacreWallpapers.advanceRotation(value);
    }
}
