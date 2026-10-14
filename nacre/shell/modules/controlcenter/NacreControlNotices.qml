import QtQuick
import QtQuick.Layouts
import qs.widgets
import qs.services
import qs.modules.notifications

Item {
    id: root
    readonly property var entries: NacreNotifs.retained
    readonly property var preview: entries.slice(-2).reverse()
    signal detailsRequested(string section, bool pointerOwned)
    implicitHeight: body.implicitHeight
    Column {
        id: body
        width: root.width
        spacing: 10
        RowLayout {
            width: parent.width
            height: 32
            spacing: 6
            NacreText {
                Layout.fillWidth: true
                Layout.minimumWidth: 0
                text: NacreI18n.tr("popouts.history-popup.text.notifications", "Notifications")
                font.pointSize: 11
                font.weight: Font.Medium
                elide: Text.ElideRight
            }
            NacreText {
                visible: root.entries.length > 0
                text: NacreI18n.number(root.entries.length)
                font.pointSize: 10
                color: NacreTokens.mutedInk
            }
            NacreControlButton {
                objectName: "controlClearNotifications"
                icon: "delete_sweep"
                label: NacreI18n.tr("dashboard.notifications-extra.clear", "Clear history")
                enabled: root.entries.length > 0
                onClicked: NacreNotifs.clearHistory()
            }
            NacreControlButton {
                objectName: "controlAllNotifications"
                icon: "chevron_right"
                label: NacreI18n.tr("controlcenter.notifications.view-all", "View all notifications")
                onClicked: root.detailsRequested("notifications", pointerActivation)
            }
        }
        NacreText {
            width: parent.width
            visible: root.entries.length === 0
            text: NacreNotifs.historyReady ? NacreI18n.tr("popouts.history-popup.text.no-notifications", "No notifications") : NacreI18n.tr("popouts.history-popup.text.loading-notifications", "Loading notifications…")
            font.pointSize: 10
            color: NacreTokens.mutedInk
        }
        Repeater {
            model: root.preview
            delegate: NacreNotice {
                objectName: "controlNotificationPreview"
                width: body.width
                history: true
            }
        }
    }
}
