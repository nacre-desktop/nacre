pragma ComponentBehavior: Bound
import QtQuick
import Quickshell.Io
import qs.widgets
import qs.services
import qs.utils

Column {
    id: root
    required property var registry
    readonly property var published: NacreSettingsStorage.documents["theme-targets"]?.targets || []
    readonly property var targets: published.length ? published : JSON.parse(manifest.text()).targets
    spacing: NacreTokens.space.sm
    FileView {
        id: manifest
        path: NacrePaths.tools + "/theme-targets.json"
        blockLoading: true
        printErrors: false
    }
    Repeater {
        model: root.targets
        delegate: NacreSurface {
            id: target
            required property var modelData
            width: root.width
            height: Math.max(title.implicitHeight, status.implicitHeight) + NacreTokens.space.xl * 2
            radius: NacreTokens.radii.small
            color: NacreTokens.body
            NacreText {
                id: title
                x: NacreTokens.space.md
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width / 2
                text: root.registry.text(target.modelData.labelId)
                wrapMode: Text.Wrap
            }
            NacreText {
                id: status
                anchors.right: parent.right
                anchors.rightMargin: NacreTokens.space.md
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width / 2 - NacreTokens.space.md * 2
                horizontalAlignment: Text.AlignRight
                wrapMode: Text.Wrap
                color: NacreTokens.mutedInk
                font.pointSize: NacreTokens.textSize.caption
                text: root.registry.text(target.modelData.status === "tool-missing" ? "settings.registry.ui.theme-missing" : target.modelData.status === "current" ? "settings.registry.ui.theme-current" : "settings.registry.ui.publisher-status")
            }
        }
    }
}
