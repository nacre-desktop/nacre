import QtQuick
import qs.widgets
import qs.services
import "../../notifications"

NacreSettingsPage {
    id: root
    property string page: "notifications"
    property bool confirmClear: false
    NacreSettingsSection {
        title: NacreI18n.tr("ui.notification-page.title.notification-history", "Notification history")
        description: NacreI18n.tr("ui.notification-page.description.value0-saved-notifications", "{value0} saved notifications", {
            value0: NacreNotifs.retained.length
        })
        Flow {
            width: parent.width
            spacing: NacreTokens.space.sm
            ActionButton {
                objectName: "requestClearHistory"
                text: root.confirmClear ? NacreI18n.tr("ui.notification-page.text.clear-saved-notifications", "Clear saved notifications") : NacreI18n.tr("ui.notification-page.text.clear-history", "Clear history")
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
                text: NacreI18n.tr("ui.notification-page.text.cancel", "Cancel")
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
            text: NacreI18n.tr("ui.notification-page.text.new-message-notifications-will-appear-here", "New message notifications will appear here.")
            wrapMode: Text.Wrap
            color: NacreTokens.mutedInk
        }
    }
}
