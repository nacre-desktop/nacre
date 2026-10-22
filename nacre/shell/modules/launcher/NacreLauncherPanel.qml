pragma ComponentBehavior: Bound
import QtQuick
import Quickshell
import qs.widgets
import qs.services
import qs.modules.extras as Extras

Item {
    id: root
    required property PersistentProperties visibilities
    property alias motionItem: motion
    property string screenName: ""
    function prepareLift() {
        completeRenderAnimations();
    }
    readonly property bool opening: visibilities.launcher && edgeOpening.running
    property bool restoreImmediately: false
    property bool motionReady: false
    Component.onCompleted: motionReady = true
    onRestoreImmediatelyChanged: if (restoreImmediately && visibilities.launcher)
        completeImmediateRestore()
    onDetachedChanged: if (detached)
        Qt.callLater(() => {
            if (root.detached)
                root.completeRenderAnimations();
        })
    function completeImmediateRestore() {
        if (!restoreImmediately || !visibilities.launcher)
            return;
        Qt.callLater(() => {
            if (!root.restoreImmediately || !root.visibilities.launcher)
                return;
            root.completeRenderAnimations();
        });
    }
    function completeRenderAnimations() {
        edgeOpening.complete();
        motionReady = false;
        slideAnimator.complete();
        motion.y = detached || revealTarget ? 0 : contentHeight;
        motion.opacity = handoffHidden ? 0 : 1;
        motion.y = Qt.binding(() => root.detached || root.revealTarget ? 0 : root.contentHeight);
        motion.opacity = Qt.binding(() => root.handoffHidden ? 0 : 1);
        motionReady = true;
    }
    Connections {
        target: root.visibilities
        function onLauncherChanged() {
            if (root.restoreImmediately && root.visibilities.launcher)
                root.completeImmediateRestore();
        }
    }
    function restoreAttached() {
        edgeOpening.complete();
        reveal = Qt.binding(() => visibilities.launcher && page.status === Loader.Ready ? 1 : 0);
        Qt.callLater(() => root.completeRenderAnimations());
    }
    property bool handoffHidden: false
    // One progress value moves the entire fixed-size launcher, including the
    // host-supplied material and search field, from the bottom edge.
    property real reveal: visibilities.launcher && page.status === Loader.Ready ? 1 : 0
    // Keep a GUI-side progress value for lifecycle and trace hooks, while the
    // rendered container itself uses scene-graph Animators for its motion.
    readonly property bool revealTarget: visibilities.launcher && page.status === Loader.Ready
    onRevealTargetChanged: if (restoreImmediately && visibilities.launcher)
        completeImmediateRestore()
    readonly property real slideOffset: detached ? 0 : (1 - reveal) * contentHeight
    readonly property bool renderAnimationRunning: slideAnimator.running
    onEnabledChanged: if (enabled) {
        handoffHidden = false;
        completeImmediateRestore();
    }
    property bool detached: false
    property bool closeAfterLaunch: false
    property var menuContext: NacreMenu
    property var pendingDaemonRow: null
    function activateFromDaemon(row) {
        if (pendingDaemonRow || (visibilities.launcher && !canDetach) || !menuContext.daemonOperationSupported(row.operation))
            return false;
        pendingDaemonRow = row;
        if (!visibilities.launcher || mode !== "apps")
            NacrePanelState.openMode("apps", "", false);
        Qt.callLater(flushDaemonActivation);
        return true;
    }
    function flushDaemonActivation() {
        if (!pendingDaemonRow || mode !== "apps" || !page.item?.run)
            return;
        const row = pendingDaemonRow;
        pendingDaemonRow = null;
        page.item.run(row);
    }
    Connections {
        target: page
        function onItemChanged() {
            Qt.callLater(root.flushDaemonActivation);
        }
    }
    signal closeRequested
    signal uiChanged
    readonly property string mode: visibilities.launcherMode || "apps"
    readonly property bool fullScreenGallery: mode === "wallpaper" && (NacreWallpapers.preferences.layout || "carousel") !== "carousel"
    readonly property real viewportWidth: Math.max(0, parent?.width ?? 0)
    readonly property real viewportHeight: Math.max(0, parent?.height ?? 0)
    readonly property real contentHeight: detached ? viewportHeight : fullScreenGallery ? Math.max(0, viewportHeight) : (page.item?.implicitHeight ?? 300)
    readonly property int galleryCount: mode === "wallpaper" ? page.item?.count ?? 0 : 0
    readonly property int galleryIndex: mode === "wallpaper" ? page.item?.currentIndex ?? -1 : -1
    readonly property real presentedHeight: reveal * contentHeight
    implicitHeight: visibilities.launcher || reveal > 0 ? contentHeight : 0
    implicitWidth: detached ? viewportWidth : fullScreenGallery ? Math.max(0, viewportWidth) : page.item?.implicitWidth ?? 800
    visible: height > 0 && (page.status === Loader.Ready || reveal > 0)
    clip: true
    onVisibleChanged: if (visible && visibilities.launcher)
        Qt.callLater(root.focusExplicit)

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
    Item {
        id: motion
        objectName: "launcherMotion"
        x: 0
        y: root.detached || root.revealTarget ? 0 : root.contentHeight
        width: root.width
        height: root.height
        opacity: root.handoffHidden ? 0 : 1
        Behavior on y {
            enabled: root.motionReady && !root.detached && (!root.restoreImmediately || !root.visibilities.launcher)
            YAnimator {
                id: slideAnimator
                duration: NacreTokens.motionEnabled ? 300 : 0
                easing.type: Easing.OutCubic
            }
        }
    }
    NacreIdleLoader {
        id: page
        parent: motion
        y: 0
        // Cache the fixed-size page while it slides behind the edge clip. Text
        // and icons then paint once rather than on every software-rendered frame.
        layer.enabled: edgeOpening.running
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
            opening: root.opening
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

    Behavior on reveal {
        enabled: !root.detached && (!root.restoreImmediately || !root.visibilities.launcher)
        NumberAnimation {
            id: edgeOpening
            duration: NacreTokens.motionEnabled ? 300 : 0
            easing.type: Easing.OutCubic
        }
    }
    Keys.onEscapePressed: if (!root.detached)
        root.visibilities.launcher = false
}
