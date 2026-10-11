import QtQuick
import qs.widgets
import qs.services

NacreSettingsPage {
    id: root
    property string page: "ai"
    Component.onCompleted: SidebarChat.start()
    function chooseProvider(provider) {
        if (["codex", "claude"].includes(provider))
            SidebarChat.setProvider(provider);
    }
    NacreSettingsSection {
        title: "Default assistant"
        description: "Applies to new chats. Existing conversations keep their assistant and account."
        Flow {
            width: parent.width
            spacing: 8
            ActionButton {
                text: "Codex"
                selected: SidebarChat.defaultProvider === "codex"
                onClicked: root.chooseProvider("codex")
            }
            ActionButton {
                text: "Claude"
                selected: SidebarChat.defaultProvider === "claude"
                onClicked: root.chooseProvider("claude")
            }
        }
    }
    NacreSettingsSection {
        title: "AI sidebar"
        NacreSettingToggle {
            label: "Enable the AI sidebar"
            setting: "leftDrawer"
        }
        ActionButton {
            text: "Open sidebar"
            enabled: DesktopSettings.data.leftDrawer !== false
            onClicked: AppLaunch.run(["nacre-shell", "left"])
        }
    }
    NacreSettingsSection {
        title: "Accounts and workspace"
        description: "Manage assistant accounts, usage and projects in the AI workspace."
        Flow {
            width: parent.width
            spacing: 8
            ActionButton {
                text: "Accounts, usage and projects"
                onClicked: AppLaunch.run(["kitty", "--class", "nacre-ai-dashboard", "--title", "Nacre AI settings", "--", "nacre-ai", "settings"])
            }
            ActionButton {
                text: "Open Brain"
                onClicked: AppLaunch.run(["nacre-shell", "brain"])
            }
        }
    }
}
