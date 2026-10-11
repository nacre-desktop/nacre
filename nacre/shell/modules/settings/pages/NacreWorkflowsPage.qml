import QtQuick
import qs.widgets
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
        title: "Desktop presets"
        Flow {
            width: parent.width
            spacing: 8
            Repeater {
                model: ["normal", "focused", "presentation", "minimal", "meeting", "music", "docked"]
                ActionButton {
                    required property string modelData
                    text: modelData.charAt(0).toUpperCase() + modelData.slice(1)
                    selected: (DesktopSettings.data.preset || "normal") === modelData
                    onClicked: DesktopSettings.request(["preset", modelData])
                }
            }
        }
    }
    NacreSettingsSection {
        title: "Personal workflow setup"
        description: "Save current audio and display choices for this preset. Only connected devices are recalled; microphone mute stays unchanged. Startup apps are opt-in."
        ActionButton {
            text: "Save current audio and display setup"
            enabled: DesktopSettings.busy !== true
            onClicked: root.save()
        }
        Flow {
            width: parent.width
            spacing: 8
            Repeater {
                model: ["Browser", "Nacre AI", "Discord", "Spotify", "Mail", "Brain"]
                ActionButton {
                    required property string modelData
                    text: modelData
                    selected: root.roles.includes(modelData)
                    onClicked: root.toggleRole(modelData)
                }
            }
        }
    }
    NacreText {
        width: parent.width
        text: DesktopSettings.message || ""
        font.pointSize: 10
        color: NacreTokens.mutedInk
        wrapMode: Text.Wrap
    }
}
