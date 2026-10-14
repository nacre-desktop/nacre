import QtQuick
import qs.widgets
import qs.services
import qs.modules.notifications

Item {
    id: root
    property bool showHeading: true
    property real maximumListHeight: 440
    readonly property var entries: NacreNotifs.retained
    implicitWidth: 380
    implicitHeight: body.implicitHeight
    function clear() {
        if (entries.length)
            NacreNotifs.clearHistory();
    }
    Column {
        id: body
        width: root.width
        spacing: 12
        Row {
            width: parent.width
            spacing: 10
            NacreText {
                width: Math.max(0, parent.width - dndToggle.width - clearButton.width - parent.spacing * 2)
                anchors.verticalCenter: parent.verticalCenter
                text: root.showHeading ? NacreI18n.tr("popouts.history-popup.text.notifications", "Notifications") : ""
                font.pointSize: 14
            }
            ActionButton {
                id: dndToggle
                objectName: "quickDnd"
                text: NacreI18n.tr("popouts.history-popup.text.dnd", "DND")
                Accessible.name: NacreNotifs.dnd ? NacreI18n.tr("popouts.history-popup.name.disable-do-not-disturb", "Disable do not disturb") : NacreI18n.tr("popouts.history-popup.name.enable-do-not-disturb", "Enable do not disturb")
                icon: NacreNotifs.dnd ? "do_not_disturb_on" : "notifications_active"
                compact: true
                selected: NacreNotifs.dnd
                onClicked: DesktopSettings.set("dnd", !NacreNotifs.dnd)
            }
            ActionButton {
                id: clearButton
                objectName: "quickClearHistory"
                text: NacreI18n.tr("popouts.history-popup.text.clear", "Clear")
                compact: true
                enabled: root.entries.length > 0
                onClicked: root.clear()
            }
        }

        NacreText {
            width: parent.width
            visible: root.entries.length === 0
            text: NacreNotifs.historyReady ? NacreI18n.tr("popouts.history-popup.text.no-notifications", "No notifications") : NacreI18n.tr("popouts.history-popup.text.loading-notifications", "Loading notifications…")
            color: NacreTokens.mutedInk
        }
        NacreQuickList {
            width: parent.width
            maximumHeight: root.maximumListHeight
            Column {
                width: parent.width
                spacing: 8
                Repeater {
                    model: root.entries
                    delegate: NacreNotice {
                        width: parent.width - 10
                        history: true
                    }
                }
            }
        }
    }
}
