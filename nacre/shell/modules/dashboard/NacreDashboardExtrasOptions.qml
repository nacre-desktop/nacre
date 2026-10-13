import QtQuick
import qs.widgets
import qs.services

Column {
    id: root
    objectName: "dashboardExtraOptions"
    spacing: 8
    Repeater {
        model: NacreDashboardState.extraOrder
        Row {
            required property string modelData
            readonly property var entry: NacreDashboardRegistry.entry(modelData)
            width: root.width
            spacing: 8
            ActionButton {
                objectName: "dashboardExtraToggle-" + parent.modelData
                width: Math.max(130, parent.width - 190)
                height: 36
                text: parent.entry?.title || ""
                icon: parent.entry?.icon || ""
                compact: true
                selected: NacreDashboardState.enabledExtras.includes(parent.modelData)
                onClicked: NacreDashboardState.toggleExtra(parent.modelData)
            }
            ActionButton {
                objectName: "dashboardExtraEarlier-" + parent.modelData
                text: NacreI18n.tr("dashboard.dashboard-customize.text.earlier", "Earlier")
                width: 82
                height: 36
                compact: true
                enabled: NacreDashboardState.extraOrder.indexOf(parent.modelData) > 0
                onClicked: NacreDashboardState.reorderExtra(parent.modelData, -1)
            }
            ActionButton {
                objectName: "dashboardExtraLater-" + parent.modelData
                text: NacreI18n.tr("dashboard.dashboard-customize.text.later", "Later")
                width: 82
                height: 36
                compact: true
                enabled: NacreDashboardState.extraOrder.indexOf(parent.modelData) < NacreDashboardState.extraOrder.length - 1
                onClicked: NacreDashboardState.reorderExtra(parent.modelData, 1)
            }
        }
    }
}
