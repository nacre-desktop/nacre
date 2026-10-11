import QtQuick
import QtQuick.Window
import Quickshell
import qs.widgets
import qs.services

FloatingWindow {
    id: root
    title: "Nacre Settings"
    objectName: "nacreSettingsWindow"
    readonly property alias view: view
    visible: NacreSettingsApp.visible
    minimumSize: Qt.size(720, 540)
    implicitWidth: Math.min(1100, Math.max(720, (screen?.width || 1920) * .75))
    implicitHeight: Math.min(800, Math.max(540, (screen?.height || 1200) * .8))
    color: NacreTokens.body
    updatesEnabled: NacreSettingsApp.active
    function activate() {
        minimized = false;
        const native = contentItem.Window.window;
        if (native)
            native.requestActivate();
        view.focusContent();
    }
    Component.onCompleted: {
        NacreSettingsApp.window = root;
        const focused = Quickshell.screens.find(s => s.name === NacreHyprland.focusedMonitor?.name);
        if (focused)
            screen = focused;
    }
    Component.onDestruction: if (NacreSettingsApp.window === root)
        NacreSettingsApp.window = null
    onClosed: NacreSettingsApp.close()
    onWindowConnected: activate()
    NacreAppHeader {
        id: header
        title: "Nacre Settings"
        app: "settings"
        width: parent.width
        section: view.current?.name || view.current?.title || ""
        onMoveRequested: root.contentItem.Window.window.startSystemMove()
    }
    NacreText {
        x: 20
        y: header.height + 8
        width: parent.width - 40
        visible: text.length > 0
        text: NacreSettingsApp.error
        color: NacreColours.palette.m3error
        wrapMode: Text.Wrap
    }
    NacreSettingsView {
        id: view
        x: 16
        y: header.height + (NacreSettingsApp.error ? 48 : 16)
        width: Math.max(0, parent.width - 32)
        height: Math.max(0, parent.height - y - 16)
        active: NacreSettingsApp.active
        onCloseRequested: NacreSettingsApp.close()
    }
    Shortcut {
        sequence: "Ctrl+W"
        context: Qt.WindowShortcut
        onActivated: NacreSettingsApp.close()
    }
    Shortcut {
        sequence: "Ctrl+F"
        context: Qt.WindowShortcut
        onActivated: view.focusSearch()
    }
}
