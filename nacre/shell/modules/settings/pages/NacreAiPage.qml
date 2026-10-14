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
        title: NacreI18n.tr("ui.ai-page.title.default-assistant", "Default assistant")
        description: NacreI18n.tr("ui.ai-page.description.applies-to-new-chats-existing-conversations-keep-their", "Applies to new chats. Existing conversations keep their assistant and account.")
        Flow {
            width: parent.width
            spacing: NacreTokens.space.sm
            ActionButton {
                text: NacreI18n.tr("ui.ai-page.text.codex", "Codex")
                selected: SidebarChat.defaultProvider === "codex"
                onClicked: root.chooseProvider("codex")
            }
            ActionButton {
                text: NacreI18n.tr("ui.ai-page.text.claude", "Claude")
                selected: SidebarChat.defaultProvider === "claude"
                onClicked: root.chooseProvider("claude")
            }
        }
    }
    NacreSettingsSection {
        title: NacreI18n.tr("ui.ai-page.title.ai-sidebar", "AI sidebar")
        ActionButton {
            text: NacreI18n.tr("settings.registry.ai.sidebar-options", "Sidebar options")
            icon: "chevron_right"
            onClicked: NacrePanelState.openSettings("panels/ai-sidebar")
        }
        ActionButton {
            text: NacreI18n.tr("ui.ai-page.text.open-sidebar", "Open sidebar")
            enabled: DesktopSettings.data.aiSidebarEnabled !== false
            onClicked: AppLaunch.run(["nacre-shell", "left"])
        }
    }
    NacreSettingsSection {
        title: NacreI18n.tr("ui.ai-page.title.accounts-and-workspace", "Accounts and workspace")
        description: NacreI18n.tr("ui.ai-page.description.manage-assistant-accounts-usage-and-projects-in-the-ai", "Manage assistant accounts, usage and projects in the AI workspace.")
        Flow {
            width: parent.width
            spacing: NacreTokens.space.sm
            ActionButton {
                text: NacreI18n.tr("ui.ai-page.text.accounts-usage-and-projects", "Accounts, usage and projects")
                onClicked: AppLaunch.run(["kitty", "--class", "nacre-ai-dashboard", "--title", "Nacre AI settings", "--", "nacre-ai", "settings"])
            }
            ActionButton {
                text: NacreI18n.tr("ui.ai-page.text.open-brain", "Open Brain")
                onClicked: AppLaunch.run(["nacre-shell", "brain"])
            }
        }
    }
}
