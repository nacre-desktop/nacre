pragma Singleton
import QtQuick
import Quickshell

Singleton {
    readonly property var core: ["today", "media", "weather", "system"]
    readonly property var names: ["today", "weather", "media", "system", "focus", "notifications", "clipboard", "devices", "network", "ai-usage", "screenshots", "brain"]
    readonly property var extras: [
        {
            id: "focus",
            title: NacreI18n.tr("dashboard.registry.focus", "Focus"),
            icon: "timer",
            page: "NacreFocusExtra.qml",
            defaultOn: false
        },
        {
            id: "notifications",
            title: NacreI18n.tr("dashboard.registry.notifications", "Notifications"),
            icon: "notifications",
            page: "NacreNotificationsExtra.qml",
            defaultOn: false
        },
        {
            id: "clipboard",
            title: NacreI18n.tr("dashboard.registry.clipboard", "Clipboard"),
            icon: "content_paste",
            page: "NacreClipboardExtra.qml",
            defaultOn: false
        },
        {
            id: "devices",
            title: NacreI18n.tr("dashboard.registry.devices", "Devices"),
            icon: "devices",
            page: "NacreDevicesExtra.qml",
            defaultOn: false
        },
        {
            id: "network",
            title: NacreI18n.tr("dashboard.registry.network", "Network"),
            icon: "wifi",
            page: "NacreNetworkExtra.qml",
            defaultOn: false
        },
        {
            id: "ai-usage",
            title: NacreI18n.tr("dashboard.registry.ai-usage", "AI usage"),
            icon: "data_usage",
            page: "NacreAiUsageExtra.qml",
            defaultOn: false
        },
        {
            id: "screenshots",
            title: NacreI18n.tr("dashboard.registry.screenshots", "Screenshots"),
            icon: "screenshot",
            page: "NacreScreenshotsExtra.qml",
            defaultOn: false
        },
        {
            id: "brain",
            title: NacreI18n.tr("dashboard.registry.brain", "Brain"),
            icon: "neurology",
            page: "NacreBrainExtra.qml",
            defaultOn: false
        }
    ]
    function entry(id) {
        return extras.find(page => page.id === id) || null;
    }
    function title(id) {
        if (id === "today")
            return NacreI18n.tr("dashboard.registry.dashboard", "Dashboard");
        if (id === "media")
            return NacreI18n.tr("dashboard.registry.media", "Media");
        if (id === "weather")
            return NacreI18n.tr("dashboard.registry.weather", "Weather");
        if (id === "system")
            return NacreI18n.tr("dashboard.registry.system", "System");
        return entry(id)?.title || "";
    }
    function icon(id) {
        return ({
                today: "dashboard",
                media: "queue_music",
                weather: "partly_cloudy_day",
                system: "speed"
            })[id] || entry(id)?.icon || "";
    }
}
