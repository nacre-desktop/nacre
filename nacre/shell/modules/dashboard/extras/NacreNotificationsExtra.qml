import QtQuick
import qs.widgets
import qs.services

NacreExtraFrame {
    id: root
    property string filter: ""
    readonly property var matches: [...NacreNotifs.retained].reverse().filter(entry => !filter || (entry.summary + " " + entry.appName + " " + entry.body).toLowerCase().includes(filter))
    title: NacreI18n.tr("dashboard.registry.notifications", "Notifications")
    description: NacreI18n.tr("dashboard.notifications-extra.description", "Recent messages, kept together.")
    icon: "notifications"
    Column {
        anchors.fill: parent
        spacing: 10
        Row {
            width: parent.width
            spacing: 8
            NacreTextField {
                width: parent.width - clear.width - 8
                height: 36
                placeholderText: NacreI18n.tr("dashboard.notifications-extra.filter", "Filter messages")
                onTextChanged: root.filter = text.trim().toLowerCase()
            }
            ActionButton {
                id: clear
                text: NacreI18n.tr("dashboard.notifications-extra.clear", "Clear history")
                compact: true
                enabled: NacreNotifs.retained.length > 0
                onClicked: NacreNotifs.clearHistory()
            }
        }
        Repeater {
            model: root.matches.slice(0, Math.max(1, Math.floor((root.height - 156) / 72)))
            NacreExtraRow {
                required property var modelData
                title: modelData.summary
                detail: modelData.appName + " · " + modelData.body
                icon: "notifications"
                action: NacreI18n.tr("dashboard.notifications-extra.dismiss", "Dismiss")
                onActivated: NacreNotifs.dismiss(modelData)
            }
        }
        NacreText {
            visible: !root.matches.length
            width: parent.width
            text: NacreI18n.tr("dashboard.notifications-extra.empty", "No messages match this view.")
            color: NacreTokens.mutedInk
            font.pointSize: 12
        }
        ActionButton {
            text: NacreI18n.tr("dashboard.notifications-extra.all", "All notification history")
            compact: true
            onClicked: NacrePanelState.openSettings("notifications")
        }
    }
}
