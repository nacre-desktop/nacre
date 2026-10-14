import QtQuick
import qs.widgets
import qs.services

NacreSurface {
    id: root
    required property var entry
    property string mode: "dark"
    property int screenshotIndex: 0
    signal closeRequested
    signal applyRequested(string mode)
    radius: 13
    color: NacreColours.palette.m3surfaceContainerLow
    Flickable {
        anchors.fill: parent
        anchors.margins: 16
        anchors.bottomMargin: root.entry.theme ? 70 : 16
        clip: true
        contentHeight: column.implicitHeight
        boundsBehavior: Flickable.StopAtBounds
        Column {
            id: column
            width: parent.width
            spacing: 12
            Row {
                width: parent.width
                spacing: 12
                NacreMenuButton {
                    icon: "chevron_left"
                    compact: true
                    text: NacreI18n.tr("software.preview.back", "Back")
                    onClicked: root.closeRequested()
                }
                NacreText {
                    width: parent.width - 100
                    text: root.entry.name || ""
                    font.pointSize: 16
                    elide: Text.ElideRight
                }
            }
            NacreThemePreview {
                visible: root.entry.theme === true
                entry: root.entry
                mode: root.mode
                width: parent.width
                height: Math.min(200, width * 0.62)
            }
            Rectangle {
                visible: !root.entry.theme && (root.entry.screenshots || []).length > 0
                width: parent.width
                height: Math.min(280, width * 9 / 16)
                color: NacreColours.palette.m3surface
                radius: 10
                Image {
                    id: screenshot
                    anchors.fill: parent
                    source: parent.visible ? root.entry.screenshots[root.screenshotIndex] : ""
                    sourceSize.width: 960
                    sourceSize.height: 640
                    asynchronous: true
                    fillMode: Image.PreserveAspectFit
                }
                NacreText {
                    anchors.centerIn: parent
                    visible: screenshot.status !== Image.Ready
                    text: screenshot.status === Image.Error ? NacreI18n.tr("software.preview.unavailable", "Preview unavailable") : NacreI18n.tr("software.preview.loading", "Loading preview…")
                    color: NacreColours.palette.m3onSurfaceVariant
                }
            }
            Row {
                spacing: 6
                visible: root.entry.theme === true || (root.entry.screenshots || []).length > 1
                NacreMenuButton {
                    compact: true
                    visible: root.entry.theme === true
                    selected: root.mode === "dark"
                    text: NacreI18n.tr("software.preview.dark", "Dark")
                    onClicked: root.mode = "dark"
                }
                NacreMenuButton {
                    compact: true
                    visible: root.entry.theme === true
                    selected: root.mode === "light"
                    text: NacreI18n.tr("software.preview.light", "Light")
                    onClicked: root.mode = "light"
                }
                Repeater {
                    model: root.entry.theme ? 0 : (root.entry.screenshots || []).length
                    NacreMenuButton {
                        required property int index
                        compact: true
                        text: String(index + 1)
                        selected: root.screenshotIndex === index
                        onClicked: root.screenshotIndex = index
                    }
                }
            }
            NacreText {
                width: parent.width
                text: root.entry.description || ""
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
            }
            NacreText {
                width: parent.width
                visible: !!root.entry.repository || !!root.entry.version
                text: [root.entry.categoryLabel, root.entry.repository, root.entry.version, root.entry.status].filter(Boolean).join(" · ")
                color: NacreColours.palette.m3onSurfaceVariant
                wrapMode: Text.Wrap
            }

            NacreMenuButton {
                visible: !!root.entry.website
                text: NacreI18n.tr("software.action.website", "Project website")
                icon: "open_in_new"
                onClicked: AppLaunch.run(["xdg-open", root.entry.website])
            }
            NacreText {
                visible: !root.entry.theme && (root.entry.screenshots || []).length > 0
                text: NacreI18n.tr("software.preview.credit", "Project screenshots from AppStream metadata")
                font.pointSize: 9
                color: NacreColours.palette.m3onSurfaceVariant
                wrapMode: Text.Wrap
                width: parent.width
            }
        }
    }
    NacreMenuButton {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.margins: 16
        visible: root.entry.theme === true
        text: NacreI18n.tr("software.preview.apply", "Apply theme")
        icon: "check"
        onClicked: root.applyRequested(root.mode)
    }
}
