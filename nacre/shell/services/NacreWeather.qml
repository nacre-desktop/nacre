pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io
import qs.utils

Singleton {
    id: root
    property var reading: ({})
    property string error: ""
    property bool stale: true
    property bool queued: false
    property bool received: false
    property string requestLocation: ""
    readonly property string desiredLocation: String(DesktopSettings.data.weatherLocation || "").trim()
    readonly property bool busy: reader.running
    readonly property string description: reading.description ? NacreI18n.option("weather-description", reading.description) : ""
    readonly property string location: reading.location || ""
    readonly property string icon: description ? NacreIcons.getWeatherIcon(reading.code) : "cloud_off"
    readonly property real temperature: reading.temperature ?? NaN
    readonly property var feelsLike: reading.feelsLike ?? null
    readonly property var high: reading.high ?? null
    readonly property var low: reading.low ?? null
    readonly property string detail: feelsLike === null ? "" : NacreI18n.tr("ui.weather.detail.feels-like-value0", "Feels like {value0}", {
        value0: degrees(feelsLike)
    })
    readonly property string range: high === null || low === null ? "" : NacreI18n.tr("weather.high-low", "High {high} · Low {low}", {
        high: degrees(high),
        low: degrees(low)
    })
    readonly property string displayTemperature: Number.isFinite(temperature) ? (stale ? NacreI18n.tr("weather.cached-temperature", "{temperature} (cached)", {
            temperature: degrees(temperature)
        }) : degrees(temperature)) : ""

    function degrees(value) {
        if (!Number.isFinite(value))
            return "";
        return DesktopSettings.data.weatherFahrenheit ? Math.round(value * 9 / 5 + 32) + "°F" : Math.round(value) + "°C";
    }
    function fail(message) {
        stale = true;
        error = String(message || NacreI18n.tr("ui.weather.error.weather-unavailable-try-again-or-set-a-city-in-settings", "Weather unavailable; try again or set a city in Settings.")).slice(0, 240);
    }
    function publish(text, cached) {
        try {
            if (text.length > 512000)
                throw new Error();
            const data = JSON.parse(text);
            if (!data || typeof data !== "object" || Array.isArray(data))
                throw new Error();
            if (data.error && !Number.isFinite(data.temperature)) {
                fail(data.error);
                return false;
            }
            if (typeof data.description !== "string" || !data.description || data.description.length > 512 || typeof data.location !== "string" || !data.location || data.location.length > 512 || !Number.isFinite(data.temperature) || data.temperature < -150 || data.temperature > 150 || !Number.isFinite(data.checked) || data.checked <= 0)
                throw new Error();
            for (const key of ["feelsLike", "high", "low"])
                if (data[key] !== undefined && data[key] !== null && (!Number.isFinite(data[key]) || data[key] < -150 || data[key] > 150))
                    throw new Error();
            if (data.hours !== undefined && (!Array.isArray(data.hours) || data.hours.length > 24 || !data.hours.every(hour => typeof hour.time === "string" && hour.time.length <= 32 && Number.isFinite(hour.temperature) && Math.abs(hour.temperature) <= 150 && (hour.rain === null || Number.isFinite(hour.rain) && hour.rain >= 0 && hour.rain <= 100))))
                throw new Error();
            if (data.days !== undefined && (!Array.isArray(data.days) || data.days.length > 10 || !data.days.every(day => typeof day.date === "string" && /^\d{4}-\d{2}-\d{2}$/.test(day.date) && Number.isFinite(day.low) && Number.isFinite(day.high) && day.low <= day.high && Math.abs(day.low) <= 150 && Math.abs(day.high) <= 150 && typeof day.sunrise === "string" && typeof day.sunset === "string")))
                throw new Error();
            if (cached && reading.checked && data.checked < reading.checked)
                return false;
            reading = JSON.parse(JSON.stringify(data));
            stale = data.stale === true || Date.now() / 1000 - data.checked > 1800;
            error = String(data.error || "").slice(0, 240);
            return true;
        } catch (failure) {
            if (!cached)
                fail("Could not read weather; last available conditions are still shown.");
            return false;
        }
    }
    function reload() {
        if (reader.running) {
            queued = true;
            return;
        }
        requestLocation = desiredLocation;
        received = false;
        reader.running = true;
    }
    onDesiredLocationChanged: reload()
    Component.onCompleted: reload()

    FileView {
        path: Quickshell.env("HOME") + "/.cache/nacre/weather.json"
        printErrors: false
        onLoaded: root.publish(text(), true)
    }
    Timer {
        interval: 1800000
        repeat: true
        running: true
        onTriggered: root.reload()
    }
    Process {
        id: reader
        objectName: "weatherReader"
        command: ["python3", NacrePaths.dashboardTools + "/weather.py"]
        stdout: StdioCollector {
            onStreamFinished: {
                if (root.requestLocation === root.desiredLocation)
                    root.received = root.publish(text, false);
            }
        }
        onExited: (code, status) => {
            if (root.requestLocation === root.desiredLocation && (code !== 0 || !root.received))
                root.fail(root.error);
            if (root.queued || root.requestLocation !== root.desiredLocation) {
                root.queued = false;
                followup.start();
            }
        }
    }
    Timer {
        id: followup
        interval: 100
        onTriggered: root.reload()
    }
    IpcHandler {
        target: "weatherStatus"
        function state(): string {
            return JSON.stringify({
                available: Number.isFinite(root.temperature),
                stale: root.stale,
                busy: root.busy,
                checked: root.reading.checked || 0,
                error: root.error
            });
        }
    }
}
