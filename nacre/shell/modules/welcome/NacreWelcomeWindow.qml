import QtQuick
import QtQuick.Window
import Quickshell
import qs.widgets
import qs.services

FloatingWindow {
    id: root
    title: "Nacre Welcome"
    visible: NacreWelcomeApp.visible
    minimumSize: Qt.size(720, 540)
    implicitWidth: Math.min(1020, Math.max(720, (screen?.width || 1920) * .64))
    implicitHeight: Math.min(760, Math.max(540, (screen?.height || 1200) * .70))
    color: NacreTokens.body
    updatesEnabled: NacreWelcomeApp.active
    readonly property var view: content.item
    function activate() {
        minimized = false;
        contentItem.Window.window?.requestActivate();
        view?.forceActiveFocus();
    }
    Component.onCompleted: {
        NacreWelcomeApp.window = root;
        const focused = Quickshell.screens.find(s => s.name === NacreCompositor.focusedMonitor?.name);
        if (focused)
            screen = focused;
    }
    Component.onDestruction: if (NacreWelcomeApp.window === root)
        NacreWelcomeApp.window = null
    onWindowConnected: activate()
    onClosed: NacreWelcomeApp.close()
    NacreAppHeader {
        title: "Nacre Welcome"
        width: parent.width
        section: ({
                welcome: "Welcome",
                colors: "Your colors",
                shortcuts: "Getting around",
                apps: "Nacre apps",
                ready: "You're ready",
                help: "Help"
            })[NacreWelcomeApp.page] || ""
        onMoveRequested: root.contentItem.Window.window.startSystemMove()
    }
    NacreIdleLoader {
        id: content
        wanted: NacreWelcomeApp.visible
        preserveState: true
        x: 24
        y: 80
        width: parent.width - 48
        height: parent.height - 96
        sourceComponent: NacreWelcomeView {}
    }
    Repeater {
        // Warm the four background-sized stills only while their demo is shown.
        // The opening/help pages do not use them; clicks in the demo reuse pixels.
        objectName: "welcomePosterPreloads"
        model: NacreWelcomeApp.active && NacreWelcomeApp.page === "colors" ? NacreWelcomeApp.demoEntries : []
        Image {
            required property var modelData
            visible: false
            source: "file://" + encodeURIComponent(modelData.poster || modelData.path).replace(/%2F/g, "/")
            asynchronous: true
            fillMode: Image.PreserveAspectCrop
            cache: true
            sourceSize: Qt.size(Math.ceil((root.screen?.width || 1920) * (root.contentItem.Window.window?.devicePixelRatio || 1)), Math.ceil((root.screen?.height || 1200) * (root.contentItem.Window.window?.devicePixelRatio || 1)))
        }
    }
    Shortcut {
        sequence: "Ctrl+W"
        context: Qt.WindowShortcut
        onActivated: NacreWelcomeApp.close()
    }
}
