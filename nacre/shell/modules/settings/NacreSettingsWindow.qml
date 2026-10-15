import QtQuick
import QtQuick.Window
import Quickshell
import qs.widgets
import qs.services

FloatingWindow {
    id: root
    title: NacreSettingsApp.nativeTitle
    objectName: "nacreSettingsWindow"
    readonly property var view: content.item
    readonly property string searchQuery: view?.query ?? content.savedState?.query ?? ""
    visible: NacreSettingsApp.visible
    minimumSize: Qt.size(NacreTokens.space.huge * 17, NacreTokens.space.huge * 15)
    implicitWidth: Math.min(1100, Math.max(720, (screen?.width || 1920) * .75))
    implicitHeight: Math.min(800, Math.max(540, (screen?.height || 1200) * .8))
    color: "transparent"
    // Qt already suppresses frames for unexposed windows. Re-enabling the
    // Quickshell update gate requests a frame while Wayland is still remapping.
    function activate() {
        minimized = false;
        const native = contentItem.Window.window;
        if (native)
            native.requestActivate();
        view?.focusContent();
    }
    Component.onCompleted: {
        NacreSettingsApp.window = root;
        const focused = Quickshell.screens.find(s => s.name === NacreCompositor.focusedMonitor?.name);
        if (focused)
            screen = focused;
    }
    Component.onDestruction: if (NacreSettingsApp.window === root)
        NacreSettingsApp.window = null
    onClosed: NacreSettingsApp.close()
    onWindowConnected: activate()
    NacreClip {
        anchors.fill: parent
        // Match the compositor geometry so its border cannot show through corners.
        radius: DesktopSettings.data.rounding ?? NacreTokens.radii.large
        color: NacreTokens.body
        NacreAppHeader {
            id: header
            title: NacreSettingsApp.windowTitle
            app: "settings"
            width: parent.width
            section: view?.current ? view.registry.text(view.current.labelId) : ""
            onMoveRequested: root.contentItem.Window.window.startSystemMove()
        }
        NacreText {
            x: NacreTokens.space.xl
            y: header.height + NacreTokens.space.sm
            width: parent.width - NacreTokens.space.xl * 2
            visible: text.length > 0
            text: NacreSettingsApp.error
            color: NacreColours.palette.m3error
            wrapMode: Text.Wrap
        }
        NacreIdleLoader {
            id: content
            wanted: NacreSettingsApp.visible
            preserveState: true
            x: NacreTokens.space.lg
            y: header.height + (NacreSettingsApp.error ? NacreTokens.space.huge + NacreTokens.space.lg : NacreTokens.space.lg)
            width: Math.max(0, parent.width - NacreTokens.space.lg * 2)
            height: Math.max(0, parent.height - y - NacreTokens.space.lg)
            sourceComponent: NacreSettingsView {
                active: NacreSettingsApp.active
                onCloseRequested: NacreSettingsApp.close()
            }
        }
    }
    Shortcut {
        sequence: "Ctrl+W"
        context: Qt.WindowShortcut
        onActivated: NacreSettingsApp.close()
    }
    Shortcut {
        sequence: "Ctrl+F"
        context: Qt.WindowShortcut
        onActivated: view?.focusSearch()
    }
}
