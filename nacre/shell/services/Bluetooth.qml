pragma Singleton
import Quickshell

// External configuration compatibility; native models belong to NacreBluetooth.
Singleton {
    readonly property bool powered: NacreBluetooth.powered
    readonly property bool discovering: NacreBluetooth.discovering
    readonly property var devices: NacreBluetooth.devices
    function refresh() {
        NacreBluetooth.refresh();
    }
}
