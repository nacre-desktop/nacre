import QtQuick
import qs.widgets
import qs.services

Column {
    id: root
    objectName: "dashboardExtraOptions"
    property bool includeCore: false
    property bool includeExtras: true
    property bool showPreview: true
    property string previewChoice: "today"
    readonly property var enabledOrder: NacreDashboardState.extraOrder.filter(id => NacreDashboardState.enabledExtras.includes(id))
    readonly property var shownOrder: [...(includeCore ? NacreDashboardState.coreTabs : []), ...(includeExtras ? enabledOrder : [])]
    readonly property var available: [...(includeCore ? NacreDashboardRegistry.core.filter(id => !NacreDashboardState.coreTabs.includes(id)) : []), ...(includeExtras ? NacreDashboardState.extraOrder.filter(id => !NacreDashboardState.enabledExtras.includes(id)) : [])]
    readonly property string previewPage: NacreDashboardState.tabs.includes(previewChoice) ? previewChoice : NacreDashboardState.tabs[0] || "today"
    spacing: 10
    function pageTitle(id) {
        return id === "today" ? NacreI18n.tr("dashboard.dashboard-customize.text.dashboard", "Dashboard") : NacreDashboardRegistry.title(id);
    }
    function toggle(id) {
        if (NacreDashboardRegistry.core.includes(id))
            NacreDashboardState.toggleTab(id);
        else
            NacreDashboardState.toggleExtra(id);
    }
    function move(id, direction) {
        if (NacreDashboardRegistry.core.includes(id)) {
            NacreDashboardState.reorder(id, direction);
            return;
        }
        const index = enabledOrder.indexOf(id), next = enabledOrder[index + direction];
        if (next)
            NacreDashboardState.reorderExtra(id, NacreDashboardState.extraOrder.indexOf(next) - NacreDashboardState.extraOrder.indexOf(id));
    }
    NacreText {
        visible: root.showPreview
        text: NacreI18n.tr("dashboard.extras.preview", "Your dashboard tabs")
        font.pointSize: 11
        color: NacreTokens.mutedInk
    }
    NacreSurface {
        objectName: "dashboardTabPreview"
        visible: root.showPreview
        width: parent.width
        height: 70
        radius: 18
        color: NacreTokens.body
        border.width: 1
        border.color: Qt.alpha(NacreTokens.accent, .22)
        NacreDashboardNavigation {
            id: preview
            objectName: "dashboardPreviewNavigation"
            x: 12
            y: 12
            width: Math.max(0, parent.width - 24)
            currentIndex: NacreDashboardRegistry.names.indexOf(root.previewPage)
            onSelected: index => root.previewChoice = NacreDashboardRegistry.names[index]
        }
    }
    NacreText {
        width: parent.width
        text: NacreI18n.tr("dashboard.extras.shown", "Shown in your dashboard")
        font.pointSize: 11
        visible: root.shownOrder.length > 0
    }
    Repeater {
        model: root.shownOrder
        Row {
            id: row
            required property string modelData
            readonly property bool core: NacreDashboardRegistry.core.includes(modelData)
            readonly property var siblings: core ? NacreDashboardState.coreTabs : root.enabledOrder
            readonly property int position: siblings.indexOf(modelData)
            width: root.width
            spacing: 8
            Item {
                width: Math.max(50, row.width - 120)
                height: 36
                NacreIcon {
                    x: 8
                    anchors.verticalCenter: parent.verticalCenter
                    text: NacreDashboardRegistry.icon(row.modelData)
                    color: NacreTokens.mutedInk
                    font.pointSize: 13
                }
                NacreText {
                    x: 34
                    anchors.verticalCenter: parent.verticalCenter
                    width: parent.width - x
                    text: root.pageTitle(row.modelData)
                    font.pointSize: 11
                    elide: Text.ElideRight
                }
            }
            ActionButton {
                objectName: "dashboardExtraToggle-" + row.modelData
                width: 32
                height: 36
                text: ""
                icon: "close"
                compact: true
                accessibleLabel: NacreI18n.tr("dashboard.extras.remove", "Remove {title}", {
                    title: NacreDashboardRegistry.title(row.modelData)
                })
                enabled: NacreDashboardState.extrasReady && (!row.core || row.siblings.length > 1)
                onClicked: root.toggle(row.modelData)
            }
            ActionButton {
                objectName: "dashboardExtraEarlier-" + row.modelData
                text: ""
                icon: "arrow_upward"
                accessibleLabel: row.core ? NacreI18n.tr("dashboard.dashboard-customize.text.earlier", "Earlier") + " · " + root.pageTitle(row.modelData) : NacreI18n.tr("dashboard.extras.move-earlier", "Move {title} earlier", {
                    title: NacreDashboardRegistry.title(row.modelData)
                })
                width: 32
                height: 36
                compact: true
                enabled: row.position > 0
                onClicked: root.move(row.modelData, -1)
            }
            ActionButton {
                objectName: "dashboardExtraLater-" + row.modelData
                text: ""
                icon: "arrow_downward"
                accessibleLabel: row.core ? NacreI18n.tr("dashboard.dashboard-customize.text.later", "Later") + " · " + root.pageTitle(row.modelData) : NacreI18n.tr("dashboard.extras.move-later", "Move {title} later", {
                    title: NacreDashboardRegistry.title(row.modelData)
                })
                width: 32
                height: 36
                compact: true
                enabled: row.position < row.siblings.length - 1
                onClicked: root.move(row.modelData, 1)
            }
        }
    }
    NacreText {
        text: NacreI18n.tr("dashboard.extras.add-pages", "Add pages")
        font.pointSize: 11
        visible: root.available.length > 0
    }
    Flow {
        width: parent.width
        spacing: 8
        Repeater {
            model: root.available
            ActionButton {
                required property string modelData
                objectName: "dashboardExtraToggle-" + modelData
                width: Math.min(implicitWidth, root.width)
                height: 36
                text: NacreDashboardRegistry.title(modelData)
                icon: "add"
                accessibleLabel: NacreI18n.tr("dashboard.extras.add", "Add {title}", {
                    title: text
                })
                compact: true
                selected: false
                enabled: NacreDashboardState.extrasReady
                onClicked: root.toggle(modelData)
            }
        }
    }
}
