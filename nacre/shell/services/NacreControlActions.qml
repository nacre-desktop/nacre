pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

Singleton {
    id: root
    property string inlineMessage: ""
    readonly property bool busy: DeviceActions.busy || inlineConnector.running
    readonly property string message: inlineMessage || DeviceActions.message
    function request(args) {
        if (busy)
            return;
        inlineMessage = "";
        DeviceActions.request(args);
    }
    property string wifiPassword: ""
    property bool wifiCancelled: false
    property var airplaneRestore: null
    function connectWifiInline(point, password = "") {
        if (typeof password !== "string" || password.length > 256 || /[\r\n]/.test(password) || busy || !NacreNetwork.wifiAvailable || !NacreNetwork.wifiEnabled || !point?.ssid || !point.bssid || !NacreNetwork.visibleNetworks.some(entry => entry.bssid === point.bssid && entry.interface === point.interface))
            return false;

        inlineMessage = NacreI18n.tr("controlcenter.network.connecting", "Connecting…");
        wifiPassword = password;
        wifiCancelled = false;
        inlineConnector.command = ["nmcli", "--ask", "--wait", "30", "--colors", "no", "device", "wifi", "connect", point.ssid, "bssid", point.bssid, ...(point.interface ? ["ifname", point.interface] : []), ...(point.hidden ? ["hidden", "yes"] : [])];
        inlineConnector.running = true;
        return true;
    }
    function cancelWifiInline() {
        wifiPassword = "";
        if (inlineConnector.running) {
            wifiCancelled = true;
            inlineConnector.signal(15);
        }
    }
    function setAirplane(enabled) {
        if (busy)
            return false;
        const restore = enabled ? null : airplaneRestore;
        if (enabled)
            airplaneRestore = {
                wifi: NacreNetwork.wifiEnabled,
                bluetooth: NacreBluetooth.powered
            };
        if (NacreBluetooth.available)
            NacreBluetooth.adapter.enabled = enabled ? false : restore?.bluetooth ?? true;
        if (NacreNetwork.wifiAvailable)
            request(["wifi-radio", enabled ? "off" : restore?.wifi === false ? "off" : "on"]);
        if (!enabled)
            airplaneRestore = null;
        return true;
    }
    Process {
        id: inlineConnector
        objectName: "inlineWifiConnector"
        stdinEnabled: true
        onStarted: {
            // nmcli reads --ask credentials from stdin without exposing argv or a file.
            write(root.wifiPassword + "\n");
            root.wifiPassword = "";
            stdinEnabled = false;
        }
        onExited: (exitCode, exitStatus) => {
            root.wifiPassword = "";
            stdinEnabled = true;
            root.inlineMessage = root.wifiCancelled || exitCode === 0 && exitStatus === 0 ? "" : NacreI18n.tr("controlcenter.network.failed", "Could not connect. Check the password and try again.");
            NacreNetwork.refresh();
        }
    }
}
