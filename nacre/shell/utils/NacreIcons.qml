pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

QtObject {
    id: root
    property string osName: "Linux"
    function getDesktopEntry(name) {
        return name ? DesktopEntries.heuristicLookup(String(name)) : null;
    }
    function getAppIcon(name, fallback) {
        const entry = getDesktopEntry(name);
        return Quickshell.iconPath(entry?.icon || fallback || "application-x-executable", true);
    }
    function getAppCategoryIcon(name, fallback) {
        const categories = getDesktopEntry(name)?.categories ?? [];
        const map = {
            AudioVideo: "play_circle",
            Audio: "music_note",
            Video: "movie",
            Development: "code",
            Education: "school",
            Game: "sports_esports",
            Graphics: "palette",
            Network: "language",
            Office: "description",
            Settings: "settings",
            System: "computer",
            Utility: "build"
        };
        for (const category of categories)
            if (map[category])
                return map[category];
        return fallback || "apps";
    }
    function getNetworkIcon(strength) {
        const s = Math.max(0, Math.min(100, Number(strength) || 0));
        return s >= 80 ? "signal_wifi_4_bar" : s >= 60 ? "network_wifi_3_bar" : s >= 40 ? "network_wifi_2_bar" : s >= 20 ? "network_wifi_1_bar" : "signal_wifi_0_bar";
    }
    function getWeatherIcon(code) {
        const c = Number(code);
        if (c === 113)
            return "clear_day";
        if (c === 116)
            return "partly_cloudy_day";
        if (c === 119 || c === 122)
            return "cloud";
        if (c === 143 || c === 248 || c === 260)
            return "foggy";
        if ([200, 386, 389, 392, 395].includes(c))
            return "thunderstorm";
        if ([179, 182, 185, 227, 230, 281, 284, 311, 314, 317, 320, 323, 326, 329, 332, 335, 338, 350, 362, 365, 368, 371, 374, 377].includes(c))
            return "weather_snowy";
        if ([176, 263, 266, 293, 296, 299, 302, 305, 308, 353, 356, 359].includes(c))
            return "rainy";
        return "cloud_off";
    }
    readonly property Process distribution: Process {
        running: true
        command: ["python3", "-c", "import platform; print(platform.freedesktop_os_release().get('PRETTY_NAME', 'Linux'))"]
        stdout: StdioCollector {
            onStreamFinished: {
                if (text.trim())
                    root.osName = text.trim();
            }
        }
    }
}
