import QtQuick
import QtQuick.Window
import Quickshell
import qs.widgets
import qs.services

FloatingWindow {
    id: root
    title: "Nacre Colors"
    readonly property var view: content.item
    visible: NacreColorsApp.visible
    minimumSize: Qt.size(760, 560)
    implicitWidth: Math.min(1100, Math.max(760, (screen?.width || 1920) * .68))
    implicitHeight: Math.min(780, Math.max(560, (screen?.height || 1200) * .70))
    color: NacreTokens.body
    updatesEnabled: NacreColorsApp.active
    function activate() {
        minimized = false;
        contentItem.Window.window?.requestActivate();
        view?.forceActiveFocus();
    }
    Component.onCompleted: {
        NacreColorsApp.window = root;
        const focused = Quickshell.screens.find(s => s.name === NacreCompositor.focusedMonitor?.name);
        if (focused)
            screen = focused;
    }
    Component.onDestruction: if (NacreColorsApp.window === root)
        NacreColorsApp.window = null
    onWindowConnected: activate()
    onClosed: NacreColorsApp.close()
    NacreAppHeader {
        id: header
        title: "Nacre Colors"
        app: "colors"
        width: parent.width
        tabs: [
            {
                id: "studio",
                name: "Studio"
            },
            {
                id: "compare",
                name: "Compare"
            },
            {
                id: "accessibility",
                name: "Accessibility"
            }
        ]
        currentTab: view?.page || content.savedState?.page || "studio"
        mode: NacreColorsApp.options.mode
        onNavigate: name => {
            if (view)
                view.page = name;
        }
        onModeChangedByUser: name => NacreColorsApp.change({
                mode: name,
                autoMode: false
            })
        onMoveRequested: root.contentItem.Window.window.startSystemMove()
    }
    NacreIdleLoader {
        id: content
        wanted: NacreColorsApp.visible
        preserveState: true
        x: 16
        y: 80
        width: parent.width - 32
        height: parent.height - 96
        sourceComponent: NacreColorsView {}
    }
    Shortcut {
        sequence: "Ctrl+W"
        context: Qt.WindowShortcut
        onActivated: NacreColorsApp.close()
    }
    Shortcut {
        sequence: "Ctrl+Return"
        context: Qt.WindowShortcut
        onActivated: NacreColorsApp.action("apply")
    }
    Shortcut {
        sequence: "Ctrl+F"
        context: Qt.WindowShortcut
        onActivated: view?.search()
    }
}
