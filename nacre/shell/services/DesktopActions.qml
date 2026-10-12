pragma Singleton
import Quickshell
import Quickshell.Io
import QtQuick

Singleton {
    id: root
    readonly property var list: [
        {
            "name": "Open wallpaper picker",
            "description": "Choose a wallpaper from your collection",
            "icon": "wallpaper",
            "action": "wallpaper",
            "aliases": ["wallpapers", "background", "wall"]
        },
        {
            "name": "Calculator",
            "description": "Calculate here; Enter copies the answer",
            "icon": "calculate",
            "action": "calculator",
            "aliases": ["calc", "math", "maths"]
        },
        {
            "name": "Open Nacre Colors",
            "description": "Preview, compare and export wallpaper palettes",
            "icon": "palette",
            "action": "colors",
            "brand": "colors",
            "aliases": ["colors", "colours", "orient", "palette"]
        },
        {
            "name": "Open Nacre Settings",
            "description": "Desktop, displays, sound and preferences",
            "icon": "settings",
            "action": "settings",
            "brand": "settings"
        },
        {
            "name": "Open Nacre AI",
            "description": "Open your assistant workspace and conversations",
            "icon": "neurology",
            "action": "tasks",
            "brand": "ai",
            "aliases": ["ai", "assistant", "ai workspace"]
        },
        {
            "name": "Open Nacre Brain",
            "description": "Browse your notes and knowledge map",
            "icon": "neurology",
            "action": "brain",
            "brand": "brain",
            "aliases": ["brain", "notes", "knowledge"]
        },
        {
            "name": "Open files",
            "description": "Browse folders and archives in your file manager",
            "icon": "folder_open",
            "action": "files",
            "aliases": ["files", "file manager", "dolphin"]
        },
        {
            "name": "Open terminal",
            "description": "Start a new terminal window",
            "icon": "terminal",
            "action": "terminal",
            "aliases": ["terminal", "kitty"]
        },
        {
            "name": "Clipboard history",
            "description": "Find and reuse copied text or images",
            "icon": "content_paste",
            "action": "clipboard",
            "aliases": ["clipboard", "paste"]
        },
        {
            "name": "Window overview",
            "description": "Find and focus an open window",
            "icon": "view_quilt",
            "action": "overview",
            "aliases": ["windows", "workspaces"]
        },
        {
            "name": "Keyboard shortcuts",
            "description": "Browse your current key bindings",
            "icon": "keyboard",
            "action": "keys",
            "aliases": ["shortcuts", "keybinds", "keys"]
        },
        {
            "name": "Open apps",
            "description": "Browse installed applications",
            "icon": "apps",
            "action": "apps",
            "aliases": ["apps", "applications", "launcher"]
        },
        {
            "name": "Open Nacre Welcome",
            "description": "Revisit the desktop tour",
            "icon": "waving_hand",
            "action": "welcome",
            "brand": "welcome",
            "aliases": ["welcome", "tour", "help"]
        },
        {
            "name": "Nacre AI: New chat",
            "description": "Start a conversation with your default assistant",
            "icon": "add_comment",
            "action": "new",
            "brand": "ai",
            "aliases": ["new chat", "ai chat", "chat"]
        },
        {
            "name": "Nacre AI: Resume chat",
            "description": "Continue your latest assistant conversation",
            "icon": "history",
            "action": "resume",
            "brand": "ai",
            "aliases": ["resume", "continue chat"]
        },
        {
            "name": "Nacre AI: Browse chats",
            "description": "Choose a saved conversation to continue",
            "icon": "forum",
            "action": "load",
            "brand": "ai",
            "aliases": ["saved chats", "load chat", "chat history"]
        },
        {
            "name": "Nacre Brain: Capture note",
            "description": "Save a thought to your private knowledge base",
            "icon": "edit_note",
            "action": "capture",
            "brand": "brain",
            "aliases": ["capture", "save note", "new note"]
        },
        {
            "name": "Wi-Fi settings",
            "description": "Manage wireless connections in Nacre Settings",
            "icon": "wifi",
            "action": "wifi",
            "aliases": ["wifi", "network", "wireless"]
        },
        {
            "name": "Bluetooth settings",
            "description": "Pair and manage devices in Nacre Settings",
            "icon": "bluetooth",
            "action": "bluetooth"
        },
        {
            "name": "Sound settings",
            "description": "Choose audio devices and volumes in Nacre Settings",
            "icon": "volume_up",
            "action": "sound",
            "aliases": ["audio", "volume", "sound"]
        },
        {
            "name": "Do not disturb",
            "description": "Toggle notification popups",
            "icon": "notifications_off",
            "action": "dnd",
            "aliases": ["dnd", "notifications"]
        },
        {
            "name": "Search Nacre Brain",
            "description": "Search notes in the sidebar",
            "icon": "neurology",
            "action": "left",
            "brand": "brain",
            "launcher": false
        },
        {
            "name": "Power menu",
            "description": "Lock, log out, restart or power off",
            "icon": "power_settings_new",
            "action": "power",
            "launcher": false
        },
        {
            "name": "Normal desktop",
            "description": "Apply your saved desktop profile",
            "icon": "desktop_windows",
            "action": "preset",
            "value": "normal",
            "launcher": false
        },
        {
            "name": "Focused desktop",
            "description": "Apply your saved desktop profile",
            "icon": "center_focus_strong",
            "action": "preset",
            "value": "focused",
            "launcher": false
        },
        {
            "name": "Presentation desktop",
            "description": "Apply your saved desktop profile",
            "icon": "present_to_all",
            "action": "preset",
            "value": "presentation",
            "launcher": false
        },
        {
            "name": "Minimal desktop",
            "description": "Apply your saved desktop profile",
            "icon": "crop_square",
            "action": "preset",
            "value": "minimal",
            "launcher": false
        },
        {
            "name": "Light theme",
            "description": "Apply light wallpaper colors",
            "icon": "light_mode",
            "action": "light",
            "launcher": false
        },
        {
            "name": "Dark theme",
            "description": "Apply dark wallpaper colors",
            "icon": "dark_mode",
            "action": "dark",
            "launcher": false
        },
        {
            name: DesktopSettings.data.animations === false ? "Enable animations" : "Disable animations",
            "description": "Change desktop animations",
            "icon": "animation",
            "action": "toggle-setting",
            "value": "animations",
            "launcher": false
        },
        {
            name: DesktopSettings.data.blur === false ? "Enable blur" : "Disable blur",
            "description": "Change desktop blur",
            "icon": "blur_on",
            "action": "toggle-setting",
            "value": "blur",
            "launcher": false
        },
        {
            name: DesktopSettings.data.shadow === false ? "Enable shadows" : "Disable shadows",
            "description": "Change desktop shadows",
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
            v.session = true;
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
