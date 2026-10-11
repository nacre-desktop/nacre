pragma Singleton
import Quickshell

Singleton {
    readonly property var palette: NacreColours.palette
    readonly property var current: NacreColours.current
    readonly property bool light: NacreColours.light
    readonly property var colourNames: NacreColours.colourNames
    readonly property var transparency: NacreColours.transparency
    function alpha(color, layer) {
        return NacreColours.alpha(color, layer);
    }
    function on(color) {
        return NacreColours.on(color);
    }
    function load(text) {
        return NacreColours.load(text);
    }
    function present() {
        return NacreColours.present();
    }
    function setMode(mode) {
        return NacreColours.setMode(mode);
    }
}
