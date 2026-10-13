import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
import qs.widgets
import qs.services
import qs.modules.dashboard as Dashboard
import qs.modules.launcher as Launcher
import qs.modules.extras as Extras
import qs.modules.controlcenter as Controls
import "geometry.js" as Geometry

FloatingWindow {
    id: root
    required property var entry
    property var dropPoint: null
    property string clientAddress: ""
    property bool closeAfterLaunch: entry.initial.closeAfterLaunch === true
    onCloseAfterLaunchChanged: NacrePearls.saveSoon()
    property bool ready: false
    property bool painted: false
    // Niri reports target geometry before its native movement spring settles.
    // Keep a lifted source visible while that first placement runs underneath it.
    property bool revealed: false
    onReadyChanged: {
        if (!ready)
            return;
        if (dropPoint?.exact && NacreCompositor.backendName === "niri" && NacrePearls.drag?.destination === entry)
            placementSettle.start();
        else
            revealed = true;
    }
    onRevealedChanged: if (revealed)
        handoffFrames.start()
    Timer {
        id: placementSettle
        interval: 400
        onTriggered: root.revealed = true
    }
    Connections {
        target: viewFlags
        function onDashboardTabChanged() {
            NacrePearls.saveSoon();
        }
        function onControlSectionChanged() {
            NacrePearls.saveSoon();
        }
    }
    property string error: ""
    readonly property var client: NacreCompositor.windows.find(value => value.title === root.title && value.pid === Quickshell.processId)
    readonly property bool presented: visible && !minimized && (!client || client.pinned || NacreCompositor.monitors.values.some(monitor => monitor.activeWorkspace?.id === client.workspace?.id))
    readonly property alias viewState: viewFlags
    readonly property alias providerView: lease
    readonly property alias bodyItem: body
    title: entry.title
    color: revealed ? NacreTokens.body : "transparent"
    implicitWidth: dropPoint?.exact ? dropPoint.width : ({
            launcher: 1000,
            dashboard: 984,
            ai: 520,
            controls: 440,
            media: 330
        })[entry.kind]
    implicitHeight: dropPoint?.exact ? dropPoint.height : ({
            launcher: 700,
            dashboard: 740,
            ai: 850,
            controls: 840,
            media: 370
        })[entry.kind]
    readonly property bool wallpaperGallery: entry.kind === "launcher" && viewFlags.launcherMode === "wallpaper" && (NacreWallpapers.preferences.layout || "carousel") !== "carousel"
    onWallpaperGalleryChanged: if (wallpaperGallery)
        galleryFit.restart()
    Timer {
        id: galleryFit
        interval: 80
        onTriggered: if (root.ready && !fitWorker.running)
            fitWorker.running = true
    }
    Process {
        id: fitWorker
        command: ["python3", (Quickshell.env("NACRE_PEARL_TOOLS") || Quickshell.env("HOME") + "/.local/share/nacre/shell/tools") + "/pearl-window.py", "fit", root.title, Quickshell.processId.toString(), "0", "0", Math.min(640, root.screen.width - 40).toString(), Math.min(root.screen.height - 100, Math.max(500, Math.min(820, root.width * .58 + 96))).toString()]
        onExited: {
            NacreCompositor.refreshWindows();
            NacrePearls.saveSoon();
        }
    }
    minimumSize: wallpaperGallery ? Qt.size(Math.min(640, screen.width - 40), Math.min(500, screen.height - 100)) : dropPoint?.exact ? Qt.size(Math.min(dropPoint.width, 400), Math.min(dropPoint.height, 300)) : entry.kind === "launcher" ? Qt.size(600, 460) : entry.kind === "media" ? Qt.size(260, 280) : Qt.size(400, 400)
    // Qt/compositor occlusion stops painting; provider leases gate background work.
    Timer {
        interval: 60
        running: true
        onTriggered: if (!root.ready)
            root.place(root.dropPoint)
    }
    NacreFrameBarrier {
        id: handoffFrames
        surface: root.contentItem.Window.window
        commit: () => {
            root.painted = true;
            NacrePearls.handedOff(root.entry);
        }
        reject: () => NacrePearls.handoffFailed(root.entry)
    }
    PersistentProperties {
        id: viewFlags
        property string windowKey: root.entry.key
        property bool launcher: root.entry.kind === "launcher"
        property bool dashboard: root.entry.kind === "dashboard"
        property bool left: root.entry.kind === "ai"
        property bool osd: root.entry.kind === "controls"
        property bool leftPinned: false
        property bool dashboardPinned: false
        property bool previewOnly: false
        property int dashboardTab: root.entry.initial.dashboardTab || 0
        property string launcherMode: root.entry.initial.mode || "apps"
        property string launcherRoute: root.entry.initial.route || "home"
        property string launcherQuery: root.entry.initial.query || ""
        property int launcherRequest: 0
        property string controlSection: root.entry.initial.section || "home"
        property string edgeMenu: ""
    }
    QtObject {
        id: lease
        property bool detached: true
        property bool dashboard: root.presented && viewFlags.dashboard
        property int dashboardTab: viewFlags.dashboardTab
        property bool mediaMini: root.presented && root.entry.kind === "media"
        property bool launcher: root.presented && viewFlags.launcher
        property string launcherMode: viewFlags.launcherMode
        property bool osd: root.presented && viewFlags.osd
        property string controlSection: viewFlags.controlSection
        property bool left: root.presented && viewFlags.left
    }
    NacreMenuContext {
        id: menu
        shared: NacreMenu
    }
    function activate() {
        minimized = false;
        if (!focusWorker.running)
            focusWorker.running = true;
        contentItem.Window.window?.requestActivate();
        if (body.item?.focusExplicit)
            body.item.focusExplicit();
        else
            body.item?.forceActiveFocus();
    }
    function place(point) {
        const monitor = Quickshell.screens.find(value => point && point.x >= value.x && point.y >= value.y && point.x <= value.x + value.width && point.y <= value.y + value.height) || screen;
        const target = point || {
            x: monitor.x + monitor.width / 2,
            y: monitor.y + (monitor.height - height) / 2 + 24
        };
        const position = point?.exact ? {
            x: point.x,
            y: point.y
        } : Geometry.place(target, {
            width: width,
            height: height
        }, {
            x: monitor.x,
            y: monitor.y,
            width: monitor.width,
            height: monitor.height
        });
        if (worker.running)
            return;
        worker.action = "place";
        worker.px = position.x;
        worker.py = position.y;
        worker.running = true;
    }
    function placement() {
        const data = client?.geometry;
        if (data && client.floating)
            return {
                exact: true,
                x: data.x,
                y: data.y,
                width: data.width,
                height: data.height
            };
        return dropPoint;
    }
    function captureState() {
        return body.item?.captureState ? Object.assign({}, body.item.captureState(), {
            closeAfterLaunch: closeAfterLaunch
        }) : {
            dashboardTab: viewFlags.dashboardTab,
            section: viewFlags.controlSection,
            mode: viewFlags.launcherMode
        };
    }
    function pin() {
        if (!worker.running) {
            worker.action = "pin";
            worker.running = true;
        }
    }
    function geometry(callback) {
        if (worker.running) {
            callback({
                error: NacreI18n.tr("pearls.pearl-window.error.window-operation-in-progress", "Window operation in progress")
            });
            return;
        }
        geometryCallback = callback;
        if (!worker.running) {
            worker.action = "geometry";
            worker.running = true;
        }
    }
    Process {
        id: focusWorker
        command: ["python3", (Quickshell.env("NACRE_PEARL_TOOLS") || Quickshell.env("HOME") + "/.local/share/nacre/shell/tools") + "/pearl-window.py", "focus", root.title, Quickshell.processId.toString()]
        onExited: {
            if (body.item?.focusExplicit)
                body.item.focusExplicit();
            else
                body.item?.forceActiveFocus();
        }
    }
    property var geometryCallback: null
    Process {
        id: worker
        property string action: "place"
        property real px: 0
        property real py: 0
        command: ["python3", (Quickshell.env("NACRE_PEARL_TOOLS") || Quickshell.env("HOME") + "/.local/share/nacre/shell/tools") + "/pearl-window.py", action, root.title, Quickshell.processId.toString(), px.toString(), py.toString(), root.ready ? "0" : root.implicitWidth.toString(), root.ready ? "0" : root.implicitHeight.toString()]
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    const value = JSON.parse(text);
                    if (value.error)
                        root.error = NacreI18n.tr("pearls.pearl-window.error.could-not-position-this-window-you-can-still-move-it-no", "Could not position this window. You can still move it normally.");
                    else {
                        root.clientAddress = value.address;
                        root.error = "";
                    }
                    if (worker.action === "geometry" && root.geometryCallback) {
                        const callback = root.geometryCallback;
                        root.geometryCallback = null;
                        callback(value);
                    }
                } catch (failure) {
                    root.error = NacreI18n.tr("pearls.pearl-window.error.could-not-read-window-position", "Could not read window position");
                }
            }
        }
        onExited: (exitCode, exitStatus) => {
            if (action === "place" && exitCode !== 0) {
                Qt.callLater(() => NacrePearls.handoffFailed(root.entry));
                return;
            }
            root.ready = true;
            NacreCompositor.refreshWindows();
            if (action === "place")
                Qt.callLater(root.activate);
        }
    }
    Component.onCompleted: {
        const target = NacrePearls.screen(entry.origin);
        if (target)
            screen = target;
    }
    onWindowConnected: place(dropPoint)
    onClosed: NacrePearls.remove(entry)
    Item {
        id: face
        anchors.fill: parent
        opacity: root.revealed ? 1 : 0
        Behavior on opacity {
            NumberAnimation {
                duration: root.dropPoint?.exact ? 0 : NacreTokens.motionEnabled ? 180 : 0
                easing.type: Easing.OutCubic
            }
        }
        Shortcut {
            sequence: "Ctrl+W"
            context: Qt.WindowShortcut
            onActivated: NacrePearls.remove(root.entry)
        }
        NacreSurface {
            anchors.fill: parent
            radius: 18
            color: NacreTokens.body
        }
        NacreText {
            anchors.bottom: parent.bottom
            anchors.horizontalCenter: parent.horizontalCenter
            text: root.error
            font.pointSize: 10
            color: NacreTokens.mutedInk
            visible: !!root.error
            z: 3
        }
        Loader {
            id: body
            anchors.fill: parent
            focus: true
            sourceComponent: ({
                    dashboard: dashboard,
                    launcher: launcher,
                    ai: ai,
                    controls: controls,
                    media: media
                })[root.entry.kind]
        }
    }
    Component {
        id: dashboard
        Dashboard.NacreDashboardPanel {
            visibilities: viewFlags
            detached: true
            hostActive: root.presented
        }
    }
    Component {
        id: launcher
        Launcher.NacreLauncherPanel {
            visibilities: viewFlags
            detached: true
            menuContext: menu
            closeAfterLaunch: root.closeAfterLaunch
            onCloseRequested: NacrePearls.remove(root.entry)
            onUiChanged: NacrePearls.saveSoon()
            Component.onCompleted: Qt.callLater(() => restoreState(root.entry.initial))
        }
    }
    Component {
        id: ai
        Extras.LeftDrawer {
            screen: root.screen
            visibilities: viewFlags
            chat: root.entry.chat
            Component.onCompleted: Qt.callLater(() => restoreState(root.entry.initial))
            detached: true
            hostActive: root.presented
        }
    }
    Component {
        id: controls
        Controls.NacreControlCenter {
            screen: root.screen
            visibility: true
            detached: true
            hostActive: root.presented
            section: viewFlags.controlSection
            onSectionRequested: name => viewFlags.controlSection = name
        }
    }
    Component {
        id: media
        Dashboard.NacreMiniPlayer {
            active: root.presented
        }
    }
}
