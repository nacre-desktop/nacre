pragma Singleton
import Quickshell
import Quickshell.Io
import QtQuick

Singleton {
    id: root
    readonly property var list: [
        {
            "name": NacreI18n.tr("ui.desktop-actions.name.open-wallpaper-picker", "Open wallpaper picker"),
            "description": NacreI18n.tr("ui.desktop-actions.description.choose-a-wallpaper-from-your-collection", "Choose a wallpaper from your collection"),
            "icon": "wallpaper",
            "action": "wallpaper",
            "aliases": ["wallpapers", "background", "wall"]
        },
        {
            "name": NacreI18n.tr("ui.desktop-actions.name.calculator", "Calculator"),
            "description": NacreI18n.tr("ui.desktop-actions.description.calculate-here-enter-copies-the-answer", "Calculate here; Enter copies the answer"),
            "icon": "calculate",
            "action": "calculator",
            "aliases": ["calc", "math", "maths"]
        },
        {
            "name": NacreI18n.tr("ui.desktop-actions.name.open-nacre-colors", "Open Nacre Colors"),
            "description": NacreI18n.tr("ui.desktop-actions.description.preview-compare-and-export-wallpaper-palettes", "Preview, compare and export wallpaper palettes"),
            "icon": "palette",
            "action": "colors",
            "brand": "colors",
            "aliases": ["colors", "colours", "orient", "palette"]
        },
        {
            "name": NacreI18n.tr("ui.desktop-actions.name.open-nacre-settings", "Open Nacre Settings"),
            "description": NacreI18n.tr("ui.desktop-actions.description.desktop-displays-sound-and-preferences", "Desktop, displays, sound and preferences"),
            "icon": "settings",
            "action": "settings",
            "brand": "settings"
        },
        {
            "name": NacreI18n.tr("ui.desktop-actions.name.open-nacre-ai", "Open Nacre AI"),
            "description": NacreI18n.tr("ui.desktop-actions.description.open-your-assistant-workspace-and-conversations", "Open your assistant workspace and conversations"),
            "icon": "neurology",
            "action": "tasks",
            "brand": "ai",
            "aliases": ["ai", "assistant", "ai workspace"]
        },
        {
            "name": NacreI18n.tr("ui.desktop-actions.name.open-nacre-brain", "Open Nacre Brain"),
            "description": NacreI18n.tr("ui.desktop-actions.description.browse-your-notes-and-knowledge-map", "Browse your notes and knowledge map"),
            "icon": "neurology",
            "action": "brain",
            "brand": "brain",
            "aliases": ["brain", "notes", "knowledge"]
        },
        {
            "name": NacreI18n.tr("ui.desktop-actions.name.open-files", "Open files"),
            "description": NacreI18n.tr("ui.desktop-actions.description.browse-folders-and-archives-in-your-file-manager", "Browse folders and archives in your file manager"),
            "icon": "folder_open",
            "action": "files",
            "aliases": ["files", "file manager", "dolphin"]
        },
        {
            "name": NacreI18n.tr("ui.desktop-actions.name.open-terminal", "Open terminal"),
            "description": NacreI18n.tr("ui.desktop-actions.description.start-a-new-terminal-window", "Start a new terminal window"),
            "icon": "terminal",
            "action": "terminal",
            "aliases": ["terminal", "kitty"]
        },
        {
            "name": NacreI18n.tr("ui.desktop-actions.name.clipboard-history", "Clipboard history"),
            "description": NacreI18n.tr("ui.desktop-actions.description.find-and-reuse-copied-text-or-images", "Find and reuse copied text or images"),
            "icon": "content_paste",
            "action": "clipboard",
            "aliases": ["clipboard", "paste"]
        },
        {
            "name": NacreI18n.tr("ui.desktop-actions.name.window-overview", "Window overview"),
            "description": NacreI18n.tr("ui.desktop-actions.description.find-and-focus-an-open-window", "Find and focus an open window"),
            "icon": "view_quilt",
            "action": "overview",
            "aliases": ["windows", "workspaces"]
        },
        {
            "name": NacreI18n.tr("ui.desktop-actions.name.keyboard-shortcuts", "Keyboard shortcuts"),
            "description": NacreI18n.tr("ui.desktop-actions.description.browse-your-current-key-bindings", "Browse your current key bindings"),
            "icon": "keyboard",
            "action": "keys",
            "aliases": ["shortcuts", "keybinds", "keys"]
        },
        {
            "name": NacreI18n.tr("ui.desktop-actions.name.open-apps", "Open apps"),
            "description": NacreI18n.tr("ui.desktop-actions.description.browse-installed-applications", "Browse installed applications"),
            "icon": "apps",
            "action": "apps",
            "aliases": ["apps", "applications", "launcher"]
        },
        {
            "name": NacreI18n.tr("ui.desktop-actions.name.open-nacre-welcome", "Open Nacre Welcome"),
            "description": NacreI18n.tr("ui.desktop-actions.description.revisit-the-desktop-tour", "Revisit the desktop tour"),
            "icon": "waving_hand",
            "action": "welcome",
            "brand": "welcome",
            "aliases": ["welcome", "tour", "help"]
        },
        {
            "name": NacreI18n.tr("ui.desktop-actions.name.nacre-ai-new-chat", "Nacre AI: New chat"),
            "description": NacreI18n.tr("ui.desktop-actions.description.start-a-conversation-with-your-default-assistant", "Start a conversation with your default assistant"),
            "icon": "add_comment",
            "action": "new",
            "brand": "ai",
            "aliases": ["new chat", "ai chat", "chat"]
        },
        {
            "name": NacreI18n.tr("ui.desktop-actions.name.nacre-ai-resume-chat", "Nacre AI: Resume chat"),
            "description": NacreI18n.tr("ui.desktop-actions.description.continue-your-latest-assistant-conversation", "Continue your latest assistant conversation"),
            "icon": "history",
            "action": "resume",
            "brand": "ai",
            "aliases": ["resume", "continue chat"]
        },
        {
            "name": NacreI18n.tr("ui.desktop-actions.name.nacre-ai-browse-chats", "Nacre AI: Browse chats"),
            "description": NacreI18n.tr("ui.desktop-actions.description.choose-a-saved-conversation-to-continue", "Choose a saved conversation to continue"),
            "icon": "forum",
            "action": "load",
            "brand": "ai",
            "aliases": ["saved chats", "load chat", "chat history"]
        },
        {
            "name": NacreI18n.tr("ui.desktop-actions.name.nacre-brain-capture-note", "Nacre Brain: Capture note"),
            "description": NacreI18n.tr("ui.desktop-actions.description.save-a-thought-to-your-private-knowledge-base", "Save a thought to your private knowledge base"),
            "icon": "edit_note",
            "action": "capture",
            "brand": "brain",
            "aliases": ["capture", "save note", "new note"]
        },
        {
            "name": NacreI18n.tr("ui.desktop-actions.name.wi-fi-settings", "Wi-Fi settings"),
            "description": NacreI18n.tr("ui.desktop-actions.description.manage-wireless-connections-in-nacre-settings", "Manage wireless connections in Nacre Settings"),
            "icon": "wifi",
            "action": "wifi",
            "aliases": ["wifi", "network", "wireless"]
        },
        {
            "name": NacreI18n.tr("ui.desktop-actions.name.bluetooth-settings", "Bluetooth settings"),
            "description": NacreI18n.tr("ui.desktop-actions.description.pair-and-manage-devices-in-nacre-settings", "Pair and manage devices in Nacre Settings"),
            "icon": "bluetooth",
            "action": "bluetooth"
        },
        {
            "name": NacreI18n.tr("ui.desktop-actions.name.sound-settings", "Sound settings"),
            "description": NacreI18n.tr("ui.desktop-actions.description.choose-audio-devices-and-volumes-in-nacre-settings", "Choose audio devices and volumes in Nacre Settings"),
            "icon": "volume_up",
            "action": "sound",
            "aliases": ["audio", "volume", "sound"]
        },
        {
            "name": NacreI18n.tr("ui.desktop-actions.name.do-not-disturb", "Do not disturb"),
            "description": NacreI18n.tr("ui.desktop-actions.description.toggle-notification-popups", "Toggle notification popups"),
            "icon": "notifications_off",
            "action": "dnd",
            "aliases": ["dnd", "notifications"]
        },
        {
            "name": NacreI18n.tr("ui.desktop-actions.name.search-nacre-brain", "Search Nacre Brain"),
            "description": NacreI18n.tr("ui.desktop-actions.description.search-notes-in-the-sidebar", "Search notes in the sidebar"),
            "icon": "neurology",
            "action": "left",
            "brand": "brain",
            "launcher": false
        },
        {
            "name": NacreI18n.tr("ui.desktop-actions.name.power-menu", "Power menu"),
            "description": NacreI18n.tr("ui.desktop-actions.description.lock-log-out-restart-or-power-off", "Lock, log out, restart or power off"),
            "icon": "power_settings_new",
            "action": "power",
            "launcher": false
        },
        {
            "name": NacreI18n.tr("ui.desktop-actions.name.normal-desktop", "Normal desktop"),
            "description": NacreI18n.tr("ui.desktop-actions.description.apply-your-saved-desktop-profile", "Apply your saved desktop profile"),
            "icon": "desktop_windows",
            "action": "preset",
            "value": "normal",
            "launcher": false
        },
        {
            "name": NacreI18n.tr("ui.desktop-actions.name.focused-desktop", "Focused desktop"),
            "description": NacreI18n.tr("ui.desktop-actions.description.apply-your-saved-desktop-profile", "Apply your saved desktop profile"),
            "icon": "center_focus_strong",
            "action": "preset",
            "value": "focused",
            "launcher": false
        },
        {
            "name": NacreI18n.tr("ui.desktop-actions.name.presentation-desktop", "Presentation desktop"),
            "description": NacreI18n.tr("ui.desktop-actions.description.apply-your-saved-desktop-profile", "Apply your saved desktop profile"),
            "icon": "present_to_all",
            "action": "preset",
            "value": "presentation",
            "launcher": false
        },
        {
            "name": NacreI18n.tr("ui.desktop-actions.name.minimal-desktop", "Minimal desktop"),
            "description": NacreI18n.tr("ui.desktop-actions.description.apply-your-saved-desktop-profile", "Apply your saved desktop profile"),
            "icon": "crop_square",
            "action": "preset",
            "value": "minimal",
            "launcher": false
        },
        {
            "name": NacreI18n.tr("ui.desktop-actions.name.light-theme", "Light theme"),
            "description": NacreI18n.tr("ui.desktop-actions.description.apply-light-wallpaper-colors", "Apply light wallpaper colors"),
            "icon": "light_mode",
            "action": "light",
            "launcher": false
        },
        {
            "name": NacreI18n.tr("ui.desktop-actions.name.dark-theme", "Dark theme"),
            "description": NacreI18n.tr("ui.desktop-actions.description.apply-dark-wallpaper-colors", "Apply dark wallpaper colors"),
            "icon": "dark_mode",
            "action": "dark",
            "launcher": false
        },
        {
            name: DesktopSettings.data.animations === false ? NacreI18n.tr("ui.desktop-actions.name.enable-animations", "Enable animations") : NacreI18n.tr("ui.desktop-actions.name.disable-animations", "Disable animations"),
            "description": NacreI18n.tr("ui.desktop-actions.description.change-desktop-animations", "Change desktop animations"),
            "icon": "animation",
            "action": "toggle-setting",
            "value": "animations",
            "launcher": false
        },
        {
            name: DesktopSettings.data.blur === false ? NacreI18n.tr("ui.desktop-actions.name.enable-blur", "Enable blur") : NacreI18n.tr("ui.desktop-actions.name.disable-blur", "Disable blur"),
            "description": NacreI18n.tr("ui.desktop-actions.description.change-desktop-blur", "Change desktop blur"),
            "icon": "blur_on",
            "action": "toggle-setting",
            "value": "blur",
            "launcher": false
        },
        {
            name: DesktopSettings.data.shadow === false ? NacreI18n.tr("ui.desktop-actions.name.enable-shadows", "Enable shadows") : NacreI18n.tr("ui.desktop-actions.name.disable-shadows", "Disable shadows"),
            "description": NacreI18n.tr("ui.desktop-actions.description.change-desktop-shadows", "Change desktop shadows"),
            "icon": "shadow",
            "action": "toggle-setting",
            "value": "shadow",
            "launcher": false
        }
    ]
    IpcHandler {
        target: "desktopActions"
        function run(action: string, value: string): void {
            root.execute(action, value);
        }
    }
    function execute(action, value) {
        const v = NacrePanelState.getForActive();
        if (!v)
            return;
        v.previewOnly = false;
        v.launcher = false;
        v.left = false;
        v.leftPinned = false;
        if (["apps", "palette", "overview", "clipboard", "keys", "wallpaper"].includes(action)) {
            NacrePanelState.openMode(action);
            return;
        }
        if (action === "calculator") {
            NacrePanelState.openMode("apps", "= ");
            return;
        }
        if (action === "colors") {
            NacreColorsApp.open();
            return;
        }
        if (action === "welcome") {
            NacreWelcomeApp.open("home");
            return;
        }
        const settingsPages = {
            settings: "appearance",
            sound: "sound",
            wifi: "network",
            bluetooth: "bluetooth"
        };
        if (Object.prototype.hasOwnProperty.call(settingsPages, action)) {
            NacrePanelState.openSettings(settingsPages[action]);
            return;
        }
        if (action === "left") {
            v.left = true;
            return;
        }
        if (action === "preset") {
            DesktopSettings.request(["preset", value]);
            return;
        }
        if (action === "toggle-setting" && ["animations", "blur", "shadow"].includes(value)) {
            DesktopSettings.set(value, !DesktopSettings.data[value]);
            return;
        }
        if (action === "dnd") {
            DesktopSettings.set("dnd", !DesktopSettings.data.dnd);
            return;
        }
        if (action === "light" || action === "dark") {
            NacreColours.setMode(action);
            return;
        }
        if (action === "power") {
            NacrePanelState.openControls("session", "", true);
            return;
        }
        const commands = {
            audio: ["pavucontrol"],
            network: ["nm-connection-editor"],
            lock: ["loginctl", "lock-session"],
            load: ["kitty", "--class", "nacre-ai-task", "--title", "Load chat", "--", "nacre-ai", "window", "--worker-command", "sessions"],
            files: ["nacre-app", "files"],
            terminal: ["nacre-app", "terminal"],
            new: ["nacre-shell", "new"],
            resume: ["nacre-shell", "resume"],
            tasks: ["nacre-shell", "tasks"],
            brain: ["nacre-shell", "brain"],
            capture: ["nacre-shell", "capture"],
            wifi: ["nacre-shell", "wifi"],
            bluetooth: ["nacre-shell", "bluetooth"]
        };
        if (commands[action])
            AppLaunch.run(commands[action]);
    }
}
