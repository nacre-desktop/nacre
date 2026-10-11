import QtQuick
import qs.widgets
import qs.services
import qs.modules.notifications

Item {
    id: root
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
                text: "Notifications"
                font.pointSize: 14
            }
            ActionButton {
                id: dndToggle
                objectName: "quickDnd"
                text: "DND"
                Accessible.name: NacreNotifs.dnd ? "Disable do not disturb" : "Enable do not disturb"
                icon: NacreNotifs.dnd ? "do_not_disturb_on" : "notifications_active"
                compact: true
                selected: NacreNotifs.dnd
                onClicked: DesktopSettings.set("dnd", !NacreNotifs.dnd)
            }
            ActionButton {
                id: clearButton
                objectName: "quickClearHistory"
                text: "Clear"
                compact: true
                enabled: root.entries.length > 0
                onClicked: root.clear()
            }
        }

        NacreText {
            width: parent.width
            visible: root.entries.length === 0
            text: NacreNotifs.historyReady ? "No notifications" : "Loading notifications…"
            color: NacreTokens.mutedInk
        }
        NacreQuickList {
            width: parent.width
            maximumHeight: 440
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
