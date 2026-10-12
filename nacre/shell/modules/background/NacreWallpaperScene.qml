import QtQuick
import QtQuick.Window
import Quickshell.Io
import qs.widgets
import qs.services
import qs.config

Item {
    id: root
    property string screenName: ""
    property url source: fileUrl(NacreWallpapers.pendingPoster)
    readonly property string requestedPath: NacrePresentation.canonicalPoster(String(source))
    property Image current: first
    property Image previous: null
    property Image standby: null
    property bool transitioning: false
    property string failure: ""
    property string displayedPath: ""
    readonly property bool hasImage: displayedPath !== "" && current.requestPath === displayedPath
    readonly property var monitor: NacreCompositor.monitors.values.find(item => item.name === screenName)
    readonly property int workspaceId: monitor?.activeWorkspace?.id ?? NacreCompositor.activeWsId
    readonly property bool pickerOpen: Object.entries(NacrePanelState.screens).some(([name, view]) => name === screenName && view.launcher && view.launcherMode === "wallpaper")
    readonly property bool covered: NacreCompositor.windows.some(client => client.workspace?.id === workspaceId && (client.fullscreen || (WallpaperPlayback.pauseCovered && !client.floating)))
    readonly property bool motionAllowed: !WallpaperPlayback.sleeping && !WallpaperPlayback.locked && !WallpaperPlayback.paused && !WallpaperPlayback.batteryPaused && (pickerOpen || !covered)
    readonly property real pixelRatio: Window.window?.devicePixelRatio || 1
    function fileUrl(path) {
        return path ? "file://" + encodeURIComponent(path).replace(/%2F/g, "/") : "";
    }
    function request() {
        if (!requestedPath || width < 1 || height < 1)
            return;
        if (hasImage && current.requestPath === requestedPath) {
            NacrePresentation.activate(requestedPath);
            return;
        }
        const slots = [first, second, third];
        const slot = !hasImage ? current : (slots.find(item => item !== current && item !== previous && item.requestPath === requestedPath) || slots.find(item => item !== current && item !== previous));
        if (!slot)
            return;
        slot.animateOpacity = false;
        slot.opacity = 0;
        slot.requestPath = requestedPath;
        slot.source = fileUrl(requestedPath);
        if (slot.status === Image.Ready)
            ready(slot);
    }
    function ready(slot) {
        if (slot.status === Image.Ready && slot.requestPath === requestedPath)
            Qt.callLater(present, slot);
        else if (slot.status === Image.Error && slot.requestPath === requestedPath)
            failure = "Wallpaper image could not be loaded; the previous image is retained.";
    }
    function present(slot) {
        if (slot.status !== Image.Ready || slot.requestPath !== requestedPath || transitioning)
            return;
        const old = hasImage ? current : null;
        if (!NacrePresentation.activate(slot.requestPath))
            return;
        failure = "";
        previous = old !== slot ? old : null;
        current = slot;
        displayedPath = slot.requestPath;
        slot.animateOpacity = !!previous;
        slot.opacity = 1;
        if (previous && NacreTokens.motionEnabled) {
            transitioning = true;
            settle.restart();
        } else
            finish();
    }
    function finish() {
        settle.stop();
        if (previous) {
            standby = previous;
            previous.animateOpacity = false;
            previous.opacity = 0;
        }
        current.animateOpacity = false;
        if (current.status === Image.Ready)
            current.opacity = 1;
        previous = null;
        transitioning = false;
        // Keep the immediately previous image warm for reverse navigation. Other
        // full-size slots are only needed while loading or fading a new request.
        for (const slot of [first, second, third]) {
            if (slot !== current && slot !== standby && slot.requestPath !== requestedPath) {
                slot.requestPath = "";
                slot.source = "";
            }
        }
        if (current.requestPath !== requestedPath)
            request();
    }
    onRequestedPathChanged: request()
    onWidthChanged: if (!displayedPath)
        Qt.callLater(request)
    onHeightChanged: if (!displayedPath)
        Qt.callLater(request)
    Component.onCompleted: request()
    Rectangle {
        anchors.fill: parent
        color: NacreTokens.body
    }
    Poster {
        id: first
    }
    Poster {
        id: second
    }
    Poster {
        id: third
    }
    AnimatedImage {
        anchors.fill: parent
        z: 2
        asynchronous: true
        cache: true
        source: NacreWallpapers.displayDynamic && NacreWallpapers.displayAnimated && !WallpaperPlayback.batteryPaused ? root.fileUrl(NacreWallpapers.displayPath) : ""
        playing: root.motionAllowed
        visible: status === Image.Ready && NacreWallpapers.displayAnimated && NacreWallpapers.displayDynamic && !WallpaperPlayback.batteryPaused
        fillMode: Image.PreserveAspectCrop
    }
    Loader {
        z: 2
        anchors.fill: parent
        active: NacreWallpapers.displayDynamic && !NacreWallpapers.displayAnimated && !WallpaperPlayback.batteryPaused
        sourceComponent: NacreDesktopVideo {
            screenName: root.screenName
            path: NacreWallpapers.displayPath
            running: root.motionAllowed
        }
    }
    Timer {
        id: settle
        interval: 230
        onTriggered: root.finish()
    }
    Connections {
        target: NacrePresentation
        function onRevisionChanged() {
            root.request();
        }
    }
    Connections {
        target: NacreTokens
        function onMotionEnabledChanged() {
            if (!NacreTokens.motionEnabled)
                root.finish();
        }
    }
    component Poster: Image {
        id: poster
        property string requestPath: ""
        property bool animateOpacity: false
        anchors.fill: parent
        asynchronous: true
        cache: true
        retainWhileLoading: true
        fillMode: Image.PreserveAspectCrop
        sourceSize.width: Math.max(1, Math.ceil(root.width * root.pixelRatio))
        sourceSize.height: Math.max(1, Math.ceil(root.height * root.pixelRatio))
        opacity: 0
        z: root.current === this ? 1 : 0
        onStatusChanged: root.ready(this)
        Behavior on opacity {
            enabled: poster.animateOpacity && NacreTokens.motionEnabled
            NumberAnimation {
                duration: 210
                easing.type: Easing.InOutCubic
            }
        }
    }
    IpcHandler {
        target: "wallpaperFrame-" + root.screenName
        function state(): string {
            return JSON.stringify({
                displayed: root.current.requestPath,
                status: root.current.status,
                pending: NacrePresentation.pending.poster || "",
                active: NacrePresentation.active.poster || "",
                color: NacreColours.palette.m3primary,
                secondary: NacreColours.palette.m3secondary,
                tertiary: NacreColours.palette.m3tertiary,
                frame: NacreFrame.colour,
                decorativeFrame: NacreColours.palette.m3frame,
                expectedFrame: NacreFrame.colour,
                paletteChoices: NacrePresentation.active.paletteOptions?.length || 0,
                selectedAccent: NacrePresentation.active.selectedAccent || "",
                motionAllowed: root.motionAllowed,
                decoderActive: NacreWallpapers.displayDynamic && !NacreWallpapers.displayAnimated && !WallpaperPlayback.batteryPaused,
                batteryPaused: WallpaperPlayback.batteryPaused,
                expected: NacrePresentation.active.poster || "",
                error: root.failure,
                transitioning: root.transitioning,
                posterSlots: [first, second, third].map(slot => ({
                            path: slot.requestPath,
                            ready: slot.status === Image.Ready,
                            width: slot.sourceSize.width,
                            height: slot.sourceSize.height
                        }))
            });
        }
    }
}
