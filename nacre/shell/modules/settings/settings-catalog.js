.pragma library

const pages = [
    {
        id: "appearance",
        labelId: "settings.catalog.label.appearance",
        label: "Appearance",
        icon: "palette",
        detailId: "settings.catalog.detail.wallpaper-colors-and-desktop-frame",
        detail: "Wallpaper, colors and desktop frame",
        terms: "wallpaper palette colors harmony rotation theme light dark workspace pearls symbols icons hints",
        kind: "appearance"
    },
    {
        id: "desktop",
        labelId: "settings.catalog.label.desktop",
        label: "Desktop",
        icon: "desktop_windows",
        detailId: "settings.catalog.detail.layout-motion-and-edge-menus",
        detail: "Layout, motion and edge menus",
        terms: "scale animation edges frame panels rounding",
        kind: "desktop"
    },
    {
        id: "displays",
        labelId: "settings.catalog.label.displays",
        label: "Displays",
        icon: "monitor",
        detailId: "settings.catalog.detail.monitors-resolution-and-scaling",
        detail: "Monitors, resolution and scaling",
        terms: "monitor screen resolution zoom scale position refresh",
        kind: "displays"
    },
    {
        id: "sound",
        labelId: "settings.catalog.label.sound",
        label: "Sound",
        icon: "volume_up",
        detailId: "settings.catalog.detail.speakers-microphones-and-application-audio",
        detail: "Speakers, microphones and application audio",
        terms: "volume speaker microphone output input mute audio",
        kind: "sound"
    },
    {
        id: "network",
        labelId: "settings.catalog.label.network",
        label: "Network",
        icon: "wifi",
        detailId: "settings.catalog.detail.connections-and-wi-fi",
        detail: "Connections and Wi-Fi",
        terms: "wifi wireless ethernet internet connection",
        kind: "network"
    },
    {
        id: "bluetooth",
        labelId: "settings.catalog.label.bluetooth",
        label: "Bluetooth",
        icon: "bluetooth",
        detailId: "settings.catalog.detail.pair-and-connect-devices",
        detail: "Pair and connect devices",
        terms: "headphones devices pair wireless",
        kind: "bluetooth"
    },
    {
        id: "notifications",
        labelId: "settings.catalog.label.notifications",
        label: "Notifications",
        icon: "notifications",
        detailId: "settings.catalog.detail.do-not-disturb-and-retained-messages",
        detail: "Do not disturb and retained messages",
        terms: "dnd history messages alerts notification",
        kind: "notifications"
    },
    {
        id: "workflows",
        labelId: "settings.catalog.label.workflows",
        label: "Workflows",
        icon: "workspaces",
        detailId: "settings.catalog.detail.workspace-roles-and-application-routes",
        detail: "Workspace roles and application routes",
        terms: "workspaces apps applications placement routing profile",
        kind: "workflows"
    },
    {
        id: "lock",
        labelId: "settings.catalog.label.lock-screen",
        label: "Lock screen",
        icon: "lock",
        detailId: "settings.catalog.detail.lock-appearance-and-idle-policy",
        detail: "Lock appearance and idle policy",
        terms: "idle sleep suspend widgets media weather lock",
        kind: "lock"
    },
    {
        id: "time",
        labelId: "settings.catalog.label.date-and-time",
        label: "Date and time",
        icon: "schedule",
        detailId: "settings.catalog.detail.time-zone-and-automatic-location",
        detail: "Time zone and automatic location",
        terms: "clock timezone date travel city weather automatic",
        kind: "time"
    },
    {
        id: "ai",
        labelId: "settings.catalog.label.nacre-ai",
        label: "Nacre AI",
        icon: "neurology",
        detailId: "settings.catalog.detail.personal-assistant-integration",
        detail: "Personal assistant integration",
        terms: "assistant codex claude brain sidebar accounts",
        kind: "ai"
    },
    {
        id: "maintenance",
        labelId: "settings.catalog.label.maintenance",
        label: "Maintenance",
        icon: "build",
        detailId: "settings.catalog.detail.checks-updates-and-recovery",
        detail: "Checks, updates and recovery",
        terms: "doctor update packages rollback release restore repair",
        kind: "maintenance"
    }
];
function find(id, translate) {
    return localize(pages.find(page => page.id === id) || pages[0], translate);
}
function search(query, translate) {
    const words = query.trim().toLowerCase().split(/\s+/);
    return localize(pages, translate).filter(page => words.every(word => (page.label + " " + page.detail + " " + page.terms).toLowerCase().includes(word)));
}

function localize(value, translate) {
    if (Array.isArray(value))
        return value.map(row => localize(row, translate));
    if (!value || typeof value !== "object")
        return value;
    const result = {};
    for (const key of Object.keys(value))
        result[key] = !key.endsWith("Id") && value[key + "Id"] && translate ? translate(value[key + "Id"], value[key]) : value[key];
    return result;
}
