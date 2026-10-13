import QtQuick
import Quickshell.Io
import qs.utils
import qs.widgets
import qs.services

NacreExtraFrame {
    id: root
    title: NacreI18n.tr("dashboard.registry.clipboard", "Clipboard")
    description: NacreI18n.tr("dashboard.clipboard-extra.description", "The things you copied, ready to use again.")
    icon: "content_paste"
    Component.onCompleted: if (shouldUpdate)
        DesktopExtras.request("clips", {})
    Process {
        running: root.shouldUpdate && root.visible
        command: ["inotifywait", "-q", "-m", "-e", "close_write,moved_to", "--format", "%f", NacrePaths.cacheRoot.slice(7) + "/cliphist"]
        stdout: SplitParser {
            onRead: name => {
                if (name === "db")
                    DesktopExtras.request("clips", {});
            }
        }
    }
    Column {
        anchors.fill: parent
        spacing: 10
        Row {
            spacing: 8
            ActionButton {
                text: NacreI18n.tr("dashboard.clipboard-extra.refresh", "Refresh")
                compact: true
                onClicked: DesktopExtras.request("clips", {})
            }
            ActionButton {
                text: NacreI18n.tr("dashboard.clipboard-extra.all", "All clipboard history")
                compact: true
                onClicked: NacrePanelState.openMode("clipboard")
            }
        }
        Repeater {
            model: DesktopExtras.clips.slice(0, Math.max(1, Math.floor((root.height - 164) / 72)))
            NacreExtraRow {
                required property var modelData
                title: modelData.title || modelData.label || ""
                detail: modelData.image ? NacreI18n.tr("dashboard.clipboard-extra.image", "Image") : NacreI18n.tr("dashboard.clipboard-extra.text", "Copied text")
                icon: modelData.image ? "image" : "content_copy"
                action: NacreI18n.tr("dashboard.clipboard-extra.copy", "Copy")
                onActivated: DesktopExtras.request("copy", {
                    id: modelData.id
                })
            }
        }
        NacreText {
            visible: !DesktopExtras.clips.length
            text: DesktopExtras.busy.clips ? NacreI18n.tr("dashboard.clipboard-extra.loading", "Reading clipboard…") : NacreI18n.tr("dashboard.clipboard-extra.empty", "Your next copy will appear here.")
            color: NacreTokens.mutedInk
            font.pointSize: 12
        }
    }
}
