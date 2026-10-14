import QtQuick
import qs.widgets
import qs.modules.dashboard as Dashboard
import qs.services

NacreSettingsPage {
    id: root
    property string page: "workflows"
    property var roles: []
    function toggleRole(role) {
        roles = roles.includes(role) ? roles.filter(item => item !== role) : [...roles, role];
    }
    function save() {
        DesktopSettings.request(["save-workflow", DesktopSettings.data.preset || "normal", JSON.stringify(roles)]);
    }
    NacreSettingsSection {
        title: NacreI18n.tr("ui.workflows-page.title.desktop-presets", "Desktop presets")
        Flow {
            width: parent.width
            spacing: NacreTokens.space.sm
            Repeater {
                model: ["normal", "focused", "presentation", "minimal", "meeting", "music", "docked"]
                ActionButton {
                    required property string modelData
                    text: NacreI18n.option("workflow", modelData)
                    selected: (DesktopSettings.data.preset || "normal") === modelData
                    onClicked: DesktopSettings.request(["preset", modelData])
                }
            }
        }
    }
    NacreSettingsSection {
        title: NacreI18n.tr("ui.workflows-page.title.personal-workflow-setup", "Personal workflow setup")
        description: NacreI18n.tr("ui.workflows-page.description.save-current-audio-and-display-choices-for-this-preset", "Save current audio and display choices for this preset. Only connected devices are recalled; microphone mute stays unchanged. Startup apps are opt-in.")
        ActionButton {
            text: NacreI18n.tr("ui.workflows-page.text.save-current-audio-and-display-setup", "Save current audio and display setup")
            enabled: DesktopSettings.busy !== true
            onClicked: root.save()
        }
        Flow {
            width: parent.width
            spacing: NacreTokens.space.sm
            Repeater {
                model: ["Browser", "Nacre AI", "Discord", "Spotify", "Mail", "Brain"]
                ActionButton {
                    required property string modelData
                    text: NacreI18n.option("workflow-role", modelData)
                    selected: root.roles.includes(modelData)
                    onClicked: root.toggleRole(modelData)
                }
            }
        }
    }
    NacreText {
        width: parent.width
        text: DesktopSettings.message || ""
        font.pointSize: NacreTokens.textSize.caption
        color: NacreTokens.mutedInk
        wrapMode: Text.Wrap
    }
}
