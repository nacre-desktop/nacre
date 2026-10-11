pragma Singleton
import Quickshell

Singleton {
    readonly property var monitors: NacreBrightness.monitors
    readonly property var ddcMonitors: NacreBrightness.ddcMonitors
    readonly property string backlightDevice: NacreBrightness.backlightDevice
    function getMonitorForScreen(screen) {
        return NacreBrightness.getMonitorForScreen(screen);
    }
    function increaseBrightness() {
        NacreBrightness.increaseBrightness();
    }
    function decreaseBrightness() {
        NacreBrightness.decreaseBrightness();
    }
}
