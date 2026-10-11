import QtQuick
import qs.widgets
import qs.services
import "../../notifications"

NacreSettingsPage {
    id: root
    property string page: "notifications"
    property bool confirmClear: false
    NacreSettingsSection {
        title: "Notification preferences"
        NacreSettingToggle {
            label: "Do not disturb"
            setting: "dnd"
        }
        NacreSettingToggle {
            label: "Show notifications on the lock screen"
            setting: "lockNotifications"
        }
        NacreSettingToggle {
            label: "Show message contents on the lock screen"
            setting: "lockNotificationContents"
        }
    }
    NacreSettingsSection {
        title: "Notification history"
        description: NacreNotifs.retained.length + " saved notifications"
        Flow {
            width: parent.width
            spacing: 8
            ActionButton {
                objectName: "requestClearHistory"
                text: root.confirmClear ? "Clear saved notifications" : "Clear history"
                enabled: NacreNotifs.retained.length > 0
                onClicked: {
                    if (root.confirmClear) {
                        NacreNotifs.clearHistory();
                        root.confirmClear = false;
                    } else
                        root.confirmClear = true;
                }
            }
            ActionButton {
                text: "Cancel"
                visible: root.confirmClear
                onClicked: root.confirmClear = false
            }
        }
        Repeater {
            model: [...NacreNotifs.retained].reverse()
            delegate: NacreNotice {
                width: parent.width
                history: true
            }
        }
        NacreText {
            width: parent.width
            visible: NacreNotifs.retained.length === 0
            text: "New message notifications will appear here."
            wrapMode: Text.Wrap
            color: NacreTokens.mutedInk
        }
    }
}
