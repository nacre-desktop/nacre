import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
import qs.widgets

FloatingWindow {
    id: root
    title: registry.text("settings.preview.title")
    objectName: "roundedSettingsPreview"
    color: "transparent"
    minimumSize: Qt.size(480, 540)
    implicitWidth: 1100
    implicitHeight: 840
    visible: true
    FontLoader {
        source: Quickshell.env("NACRE_SETTINGS_PREVIEW_ICON_FONT")
    }
    SettingsRegistry {
        id: registry
    }
    SettingsCapabilities {
        id: capabilities
        battery: true
        wifi: true
        bluetooth: true
        ai: true
        brain: true
        nightLight: true
        compositor: ({
                monitorConfiguration: true,
                windowCapture: true
            })
    }
    SettingsStore {
        id: store
        schema: registry.schema
        capabilities: capabilities.snapshot
        availableStores: ["desktop.json", "wallpaper-picker.json", "state:scheme.json", "dashboard.json", "control-center.json", "launcher.json"]
        profileValues: ({
                "panels.lips.detachablePanels": true
            })
        onActionRequested: action => {
            if (action === "colors-studio")
                view.feedback = registry.text("settings.registry.ui.studio-preview");
        }
    }
    PreviewBackend {
        id: backend
        store: store
        path: Quickshell.env("NACRE_SETTINGS_PREVIEW_STATE")
    }
    Component.onCompleted: {
        SettingsPreviewContext.store = store;
        SettingsPreviewContext.backend = backend;
    }
    NacreClip {
        id: surface
        anchors.fill: parent
        radius: NacreTokens.radii.large
        color: NacreTokens.body
        Item {
            id: titlebar
            width: parent.width
            height: 56
            MouseArea {
                anchors.fill: parent
                onPressed: root.contentItem.Window.window.startSystemMove()
            }
            NacreIcon {
                x: NacreTokens.space.xl
                anchors.verticalCenter: parent.verticalCenter
                text: "settings"
            }
            NacreText {
                x: 52
                anchors.verticalCenter: parent.verticalCenter
                text: registry.text("settings.registry.ui.preview")
                width: parent.width - x - 110
                wrapMode: Text.Wrap
                color: NacreTokens.mutedInk
                font.pointSize: NacreTokens.textSize.caption
            }
            ActionButton {
                anchors.right: parent.right
                anchors.rightMargin: NacreTokens.space.xl
                anchors.verticalCenter: parent.verticalCenter
                text: registry.text("settings.registry.ui.close")
                icon: "close"
                compact: true
                onClicked: Qt.quit()
            }
        }
        SettingsView {
            id: view
            x: NacreTokens.space.xl
            y: titlebar.height
            width: parent.width - 2 * x
            height: parent.height - y - NacreTokens.space.xl
            registry: registry
            store: store
            screen: Quickshell.screens[0]
            capabilities: capabilities.snapshot
            onCloseRequested: Qt.quit()
        }
    }
    IpcHandler {
        target: "settingsPreview"
        function open(route: string): bool {
            return view.open(route);
        }
        function search(query: string): void {
            view.query = query;
            view.focusSearch();
        }
        function size(width: int, height: int): void {
            root.implicitWidth = width;
            root.implicitHeight = height;
        }
        function capability(name: string, enabled: bool): bool {
            if (!["battery", "ai", "brain", "wifi", "bluetooth", "packages"].includes(name))
                return false;
            capabilities[name] = enabled;
            return true;
        }
        function write(id: string, value: string): bool {
            try {
                return store.write({
                    id: id
                }, JSON.parse(value));
            } catch (error) {
                return false;
            }
        }
        function scroll(fraction: real): void {
            const flick = view.findRow(view, "registryScroll");
            flick.contentY = Math.max(0, flick.contentHeight - flick.height) * Math.max(0, Math.min(1, fraction));
        }
        function capture(path: string): void {
            surface.grabToImage(result => result.saveToFile(path));
        }
        function state(): string {
            return JSON.stringify({
                page: view.page,
                query: view.query,
                highlight: view.highlight,
                results: view.results,
                width: root.width,
                height: root.height,
                writable: backend.writable,
                files: store.files,
                error: store.error
            });
        }
    }
}
