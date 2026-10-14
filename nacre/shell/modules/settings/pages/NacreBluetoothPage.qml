import QtQuick
import qs.widgets
import qs.services
import qs.modules.bar.popouts as Connectivity

NacreSettingsPage {
    id: root
    property string page: "bluetooth"
    function power() {
        if (!DeviceActions.busy && NacreBluetooth.available)
            DeviceActions.request(["bluetooth-power", NacreBluetooth.powered ? "off" : "on"]);
    }
    function deviceAction(device, action) {
        if (DeviceActions.busy || !NacreBluetooth.available || !NacreBluetooth.devices.includes(device) || !["connect", "disconnect", "trust", "untrust"].includes(action))
            return;
        DeviceActions.request(["bluetooth-" + action, device.address]);
    }
    NacreSettingsSection {
        title: NacreI18n.tr("ui.bluetooth-page.title.bluetooth", "Bluetooth")
        Connectivity.NacreBluetoothPopup {
            width: parent.width
            showHeading: false
            maximumListHeight: NacreTokens.space.huge * 14
        }
    }
}
