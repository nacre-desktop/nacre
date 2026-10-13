import QtQuick
import qs.widgets
import qs.services

NacreExtraFrame {
    id: root
    title: NacreI18n.tr("dashboard.registry.brain", "Brain")
    description: NacreI18n.tr("dashboard.brain-extra.description", "Recent notes and the next things to do.")
    icon: "neurology"
    Component.onCompleted: if (shouldUpdate)
        NacreDashboardState.request("context")
    Row {
        anchors.fill: parent
        spacing: 14
        Column {
            width: (parent.width - 14) / 2
            spacing: 10
            NacreText {
                text: NacreI18n.tr("dashboard.brain-extra.tasks", "Open tasks")
                color: NacreTokens.accent
                font.pointSize: 14
            }
            Repeater {
                model: (NacreDashboardState.brain.tasks || []).slice(0, Math.max(1, Math.floor((root.height - 160) / 72)))
                NacreExtraRow {
                    required property var modelData
                    title: modelData.text
                    detail: ""
                    icon: "check_circle"
                    action: NacreI18n.tr("dashboard.brain-extra.open", "Open")
                    onActivated: NacreDashboardState.openNote(modelData.path)
                }
            }
            NacreText {
                width: parent.width
                visible: !(NacreDashboardState.brain.tasks || []).length
                text: NacreI18n.tr("dashboard.brain-extra.no-tasks", "No open tasks in recent notes.")
                font.pointSize: 10
                color: NacreTokens.mutedInk
                wrapMode: Text.Wrap
            }
        }
        Column {
            width: (parent.width - 14) / 2
            spacing: 10
            NacreText {
                text: NacreI18n.tr("dashboard.brain-extra.notes", "Recent notes")
                color: NacreTokens.accent
                font.pointSize: 14
            }
            Repeater {
                model: (NacreDashboardState.brain.recent || []).slice(0, Math.max(1, Math.floor((root.height - 160) / 72)))
                NacreExtraRow {
                    required property var modelData
                    title: modelData.title
                    detail: ""
                    action: NacreI18n.tr("dashboard.brain-extra.open", "Open")
                    onActivated: NacreDashboardState.openNote(modelData.path)
                }
            }
            NacreText {
                width: parent.width
                visible: !(NacreDashboardState.brain.recent || []).length
                text: NacreI18n.tr("dashboard.brain-extra.no-notes", "Your recent Brain notes will appear here.")
                font.pointSize: 10
                color: NacreTokens.mutedInk
                wrapMode: Text.Wrap
            }
            ActionButton {
                text: NacreI18n.tr("dashboard.brain-extra.refresh", "Refresh")
                compact: true
                onClicked: NacreDashboardState.request("context")
            }
        }
    }
}
