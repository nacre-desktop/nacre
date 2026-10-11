pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io
import qs.utils
import "colour-data.js" as Data

Singleton {
    id: root
    property var state: Data.prepare({
        mode: "dark",
        colours: Data.fallback
    })
    readonly property var palette: state.palette
    readonly property var current: palette
    readonly property bool light: state.light
    readonly property var colourNames: Data.names
    readonly property var transparency: ({
            enabled: false,
            base: 1,
            layers: 1
        })
    property bool publishing: false
    property bool ready: false
    property string error: ""
    property int revision: 0
    function apply(data) {
        const candidate = Data.prepare(data);
        if (!candidate) {
            error = "Palette data is incomplete or invalid; the last palette is still shown.";
            return false;
        }
        if (ready && JSON.stringify(state.raw) === JSON.stringify(data)) {
            error = "";
            return true;
        }
        publishing = true;
        state = candidate;
        ready = true;
        revision++;
        error = "";
        Qt.callLater(() => publishing = false);
        return true;
    }
    function load(text) {
        return apply(Data.textPayload(text));
    }
    function present() {
        const active = NacrePresentation.active;
        if (active?.colours)
            return apply({
                mode: active.mode,
                colours: active.colours
            });
        return false;
    }
    function setMode(mode) {
        if (!["light", "dark", "auto"].includes(mode))
            return false;
        AppLaunch.run(["nacre-shell", "scheme-mode", mode]);
        return true;
    }
    function alpha(value: color, layer: bool): color {
        return value;
    }
    function on(color: color): color {
        function linear(value) {
            return value <= .04045 ? value / 12.92 : Math.pow((value + .055) / 1.055, 2.4);
        }
        return .2126 * linear(color.r) + .7152 * linear(color.g) + .0722 * linear(color.b) > .179 ? Qt.rgba(0, 0, 0, 1) : Qt.rgba(1, 1, 1, 1);
    }
    Component.onCompleted: present()
    Connections {
        target: NacrePresentation
        function onActiveChanged() {
            root.present();
        }
    }
    FileView {
        path: NacrePaths.state + "/scheme/current.txt"
        watchChanges: true
        printErrors: false
        onFileChanged: reload()
        onLoaded: if (!NacrePresentation.available)
            root.load(text())
    }
    IpcHandler {
        target: "paletteState"
        function state(): string {
            return JSON.stringify({
                ready: root.ready,
                light: root.light,
                revision: root.revision,
                error: root.error,
                surface: String(root.palette.m3surface),
                primary: String(root.palette.m3primary),
                secondary: String(root.palette.m3secondary),
                foreground: String(root.palette.m3onSurface)
            });
        }
    }
}
