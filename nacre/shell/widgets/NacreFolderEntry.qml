import QtQuick
import qs.utils
import QtQuick.Dialogs
import qs.services

Column {
    id: root
    property string value: ""
    property string label: ""
    readonly property string folderPath: value.startsWith("~/") ? NacrePaths.home.slice(7) + value.slice(1) : value || NacrePaths.pictures.slice(7) + "/Wallpapers"
    signal selected(string path)
    spacing: NacreTokens.space.sm
    NacreTextField {
        width: parent.width
        text: root.value
        placeholderText: root.folderPath
        accessibleName: root.label
        onEditingFinished: root.selected(text)
    }
    Row {
        anchors.right: parent.right
        spacing: NacreTokens.space.sm
        ActionButton {
            icon: "folder"
            text: NacreI18n.tr("settings.registry.ui.browse", "Browse…")
            compact: true
            opacity: 1
            onClicked: chooser.open()
        }
        ActionButton {
            icon: "folder_open"
            text: NacreI18n.tr("settings.registry.ui.open-folder", "Open folder")
            compact: true
            opacity: 1
            enabled: !!root.folderPath
            onClicked: AppLaunch.run(["nacre-app", "files", root.folderPath])
        }
    }
    FolderDialog {
        id: chooser
        title: root.label
        currentFolder: "file://" + root.folderPath.split("/").map(encodeURIComponent).join("/")
        onAccepted: root.selected(decodeURIComponent(String(selectedFolder).replace(/^file:\/\//, "")))
    }
}
