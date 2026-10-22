pragma Singleton
import Quickshell
import Quickshell.Io
import qs.utils

Singleton {
    id: root
    property string message: ""
    property string lastAction: ""
    readonly property bool busy: worker.running || connector.running
    function request(args) {
        if (busy)
            return;
        lastAction = args[0];
        message = NacreI18n.tr("ui.device-actions.message.working", "Working…");
        worker.command = [NacrePaths.helperBinary, "device-actions", ...args];
        worker.running = true;
    }
    function connectWifi(ssid) {
        if (busy)
            return;
        lastAction = "wifi-connect";
        message = NacreI18n.tr("ui.device-actions.message.finish-connecting", "Finish connecting in the Wi-Fi window.");
        connector.command = ["systemd-run", "--user", "--scope", "--collect", "--quiet", "env", "-u", "LD_LIBRARY_PATH", "-u", "QML_IMPORT_PATH", "-u", "QML2_IMPORT_PATH", "-u", "QT_PLUGIN_PATH", "-u", "NACRE_LIB_DIR", "kitty", "--title", "Connect to Wi-Fi", NacrePaths.helperBinary, "device-actions", "wifi-connect", ssid];
        connector.running = true;
    }
    Process {
        id: connector
        objectName: "wifiConnector"
        onExited: (exitCode, exitStatus) => {
            root.message = exitCode === 0 && exitStatus === 0 ? "" : NacreI18n.tr("ui.device-actions.message.connection-window-failed", "The Wi-Fi window closed or could not connect. Try again or open the connection editor.");
            NacreNetwork.refresh();
        }
    }
    Process {
        id: worker
        onExited: {
            if (root.lastAction.startsWith("bluetooth"))
                NacreBluetooth.refresh();
            else if (root.lastAction.startsWith("wifi"))
                NacreNetwork.refresh();
        }
        stdout: SplitParser {
            splitMarker: ""
            onRead: line => {
                try {
                    const data = JSON.parse(line);
                    root.message = data.error ?? data.message;
                } catch (e) {
                    root.message = NacreI18n.tr("ui.device-actions.message.could-not-read-device-status", "Could not read device status");
                }
            }
        }
    }
}
