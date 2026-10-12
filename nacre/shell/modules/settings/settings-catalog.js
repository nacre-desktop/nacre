.pragma library

const pages = [
    {
        id: "appearance",
        label: "Appearance",
        icon: "palette",
        detail: "Wallpaper, colors and desktop frame",
        terms: "wallpaper palette colors harmony rotation theme light dark workspace pearls symbols icons hints",
        kind: "appearance"
    },
    {
        id: "desktop",
        label: "Desktop",
        icon: "desktop_windows",
        detail: "Layout, motion and edge menus",
        terms: "scale animation edges frame panels rounding",
        kind: "desktop"
    },
    {
        id: "displays",
        label: "Displays",
        icon: "monitor",
        detail: "Monitors, resolution and scaling",
        terms: "monitor screen resolution zoom scale position refresh",
        kind: "displays"
    },
    {
        id: "sound",
        label: "Sound",
        icon: "volume_up",
        detail: "Speakers, microphones and application audio",
        terms: "volume speaker microphone output input mute audio",
        kind: "sound"
    },
    {
        id: "network",
        label: "Network",
        icon: "wifi",
        detail: "Connections and Wi-Fi",
        terms: "wifi wireless ethernet internet connection",
        kind: "network"
    },
    {
        id: "bluetooth",
        label: "Bluetooth",
        icon: "bluetooth",
        detail: "Pair and connect devices",
        terms: "headphones devices pair wireless",
        kind: "bluetooth"
    },
    {
        id: "notifications",
        label: "Notifications",
        icon: "notifications",
        detail: "Do not disturb and retained messages",
        terms: "dnd history messages alerts notification",
        kind: "notifications"
    },
    {
        id: "workflows",
        label: "Workflows",
        icon: "workspaces",
        detail: "Workspace roles and application routes",
        terms: "workspaces apps applications placement routing profile",
        kind: "workflows"
    },
    {
        id: "lock",
        label: "Lock screen",
        icon: "lock",
        detail: "Lock appearance and idle policy",
        terms: "idle sleep suspend widgets media weather lock",
        kind: "lock"
    },
    {
        id: "time",
        label: "Date and time",
        icon: "schedule",
        detail: "Time zone and automatic location",
        terms: "clock timezone date travel city weather automatic",
        kind: "time"
    },
    {
        id: "ai",
        label: "Nacre AI",
        icon: "neurology",
        detail: "Personal assistant integration",
        terms: "assistant codex claude brain sidebar accounts",
        kind: "ai"
    },
    {
        id: "maintenance",
        label: "Maintenance",
        icon: "build",
        detail: "Checks, updates and recovery",
        terms: "doctor update packages rollback release restore repair",
        kind: "maintenance"
    }
];
function find(id) {
    return pages.find(page => page.id === id) || pages[0];
}
function search(query) {
    const words = query.trim().toLowerCase().split(/\s+/);
    return pages.filter(page => words.every(word => (page.label + " " + page.detail + " " + page.terms).toLowerCase().includes(word)));
}
