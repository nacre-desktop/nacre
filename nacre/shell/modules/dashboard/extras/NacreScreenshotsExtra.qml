import QtQuick
import Qt.labs.folderlistmodel
import Quickshell
import qs.utils
import qs.widgets
import qs.services

NacreExtraFrame {
    id: root
    title: NacreI18n.tr("dashboard.registry.screenshots", "Screenshots")
    description: NacreI18n.tr("dashboard.screenshots-extra.description", "Recent captures. Copy, open or move one to the trash.")
    icon: "screenshot"
    FolderListModel {
        id: files
        folder: root.shouldUpdate ? NacrePaths.pictures + "/Screenshots" : ""
        nameFilters: ["*.png", "*.jpg", "*.webp"]
        showDirs: false
        sortField: FolderListModel.Time
        sortReversed: true
    }
    Grid {
        anchors.fill: parent
        columns: parent.width > 650 ? 3 : 2
        spacing: 12
        Repeater {
            model: Math.min(files.count, 6)
            NacreSurface {
                required property int index
                readonly property string path: files.get(index, "filePath")
                width: (parent.width - (parent.columns - 1) * parent.spacing) / parent.columns
                height: (parent.height - (parent.columns === 3 ? 12 : 24)) / (parent.columns === 3 ? 2 : 3)
                radius: 14
                color: NacreTokens.body
                clip: true
                Image {
                    x: 8
                    y: 8
                    width: parent.width - 16
                    height: parent.height - 67
                    source: files.get(parent.index, "fileUrl")
                    sourceSize.width: 320
                    sourceSize.height: 200
                    asynchronous: true
                    fillMode: Image.PreserveAspectCrop
                }
                NacreText {
                    x: 10
                    y: parent.height - 54
                    width: parent.width - 20
                    text: files.get(parent.index, "fileName")
                    font.pointSize: 8
                    elide: Text.ElideMiddle
                }
                Row {
                    x: 8
                    anchors.bottom: parent.bottom
                    anchors.bottomMargin: 6
                    spacing: 5
                    ActionButton {
                        text: ""
                        icon: "content_copy"
                        compact: true
                        accessibleLabel: NacreI18n.tr("dashboard.screenshots-extra.copy", "Copy screenshot")
                        onClicked: Quickshell.execDetached(["sh", "-c", "wl-copy --type image/png < \"$1\"", "nacre-capture", parent.parent.path])
                    }
                    ActionButton {
                        text: ""
                        icon: "open_in_new"
                        compact: true
                        accessibleLabel: NacreI18n.tr("dashboard.screenshots-extra.open", "Open screenshot")
                        onClicked: Quickshell.execDetached(["xdg-open", parent.parent.path])
                    }
                    ActionButton {
                        text: ""
                        icon: "delete"
                        compact: true
                        accessibleLabel: NacreI18n.tr("dashboard.screenshots-extra.trash", "Move screenshot to trash")
                        onClicked: Quickshell.execDetached(["gio", "trash", parent.parent.path])
                    }
                }
            }
        }
    }
    NacreText {
        anchors.centerIn: parent
        visible: !files.count
        width: parent.width - 20
        text: NacreI18n.tr("dashboard.screenshots-extra.empty", "Captures saved to Pictures/Screenshots will appear here.")
        font.pointSize: 14
        wrapMode: Text.Wrap
        horizontalAlignment: Text.AlignHCenter
        color: NacreTokens.mutedInk
    }
}
