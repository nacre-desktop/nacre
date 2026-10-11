pragma Singleton
import Quickshell
import Quickshell.Bluetooth as Bluez

Singleton {
    id: root
    property int refreshRevision: 0
    readonly property var adapter: Bluez.Bluetooth.defaultAdapter
    readonly property bool available: !!adapter
    readonly property bool powered: available && adapter.enabled
    readonly property bool discovering: available && adapter.discovering
    readonly property var devices: {
        const revision = refreshRevision;
        return [...Bluez.Bluetooth.devices.values].map(device => ({
                    name: device.deviceName || device.name || device.address,
                    alias: device.name || device.deviceName || device.address,
                    address: device.address,
                    icon: device.icon || "bluetooth",
                    connected: device.connected,
                    paired: device.paired,
                    trusted: device.trusted
                })).sort((left, right) => Number(right.connected) - Number(left.connected) || left.alias.localeCompare(right.alias) || left.address.localeCompare(right.address));
    }
    // Native BlueZ signals already update views; this only invalidates cached callers.
    function refresh() {
        refreshRevision++;
    }
}
