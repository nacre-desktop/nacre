pragma Singleton
import QtQuick

QtObject {
    readonly property string osName: NacreIcons.osName
    function getDesktopEntry(name) {
        return NacreIcons.getDesktopEntry(name);
    }
    function getAppIcon(name, fallback) {
        return NacreIcons.getAppIcon(name, fallback);
    }
    function getAppCategoryIcon(name, fallback) {
        return NacreIcons.getAppCategoryIcon(name, fallback);
    }
    function getNetworkIcon(strength) {
        return NacreIcons.getNetworkIcon(strength);
    }
    function getWeatherIcon(code) {
        return NacreIcons.getWeatherIcon(code);
    }
}
