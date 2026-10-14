import QtQuick
import QtQuick.Layouts
import qs.widgets
import qs.services

Column {
    spacing: 18
    NacreText {
        width: parent.width
        text: Updates.count ? NacreI18n.tr("status.peek.update-count", "{count} updates available", {
            count: Updates.count
        }) : NacreI18n.tr("status.peek.up-to-date", "Up to date")
        font.pointSize: 18
        wrapMode: Text.Wrap
    }
    NacreText {
        width: parent.width
        text: Updates.message || ""
        visible: text !== ""
        wrapMode: Text.Wrap
        color: NacreTokens.mutedInk
    }
    RowLayout {
        width: parent.width
        spacing: 8
        ActionButton {
            Layout.fillWidth: true
            text: NacreI18n.tr("controlcenter.updates.check", "Check for updates")
            icon: "refresh"
            onClicked: Updates.refresh()
        }
        ActionButton {
            Layout.fillWidth: true
            text: NacreI18n.tr("controlcenter.updates.open", "Open updater")
            icon: "package_2"
            onClicked: AppLaunch.run(["nacre-shell", "updates"])
        }
    }
}
