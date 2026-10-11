pragma Singleton
import Quickshell

Singleton {
    readonly property var icon: NacreWeather.icon
    readonly property var description: NacreWeather.description
    readonly property var temperature: NacreWeather.temperature
    readonly property var feelsLike: NacreWeather.feelsLike
    readonly property var high: NacreWeather.high
    readonly property var low: NacreWeather.low
    readonly property var detail: NacreWeather.detail
    readonly property var range: NacreWeather.range
    readonly property var location: NacreWeather.location
    readonly property var error: NacreWeather.error
    readonly property var stale: NacreWeather.stale
    readonly property var displayTemperature: NacreWeather.displayTemperature
    function degrees(value) {
        return NacreWeather.degrees(value);
    }
    function reload() {
        return NacreWeather.reload();
    }
}
