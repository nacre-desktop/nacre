pragma Singleton
import Quickshell
import Quickshell.Io

Singleton {
    id: root
    property string message: ""
    property string lastAction: ""
    readonly property bool busy: worker.running
    function request(args) {
        if (busy)
            return;
        lastAction = args[0];
        message = "Working…";
        worker.command = ["python3", Quickshell.env("HOME") + "/.local/share/nacre/shell/tools/device-actions.py", ...args];
        worker.running = true;
    }
    function connectWifi(ssid) {
        AppLaunch.run(["kitty", "--title", "Connect to Wi-Fi", "python3", Quickshell.env("HOME") + "/.local/share/nacre/shell/tools/device-actions.py", "wifi-connect", ssid]);
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
                    root.message = "Could not read device status";
                }
            }
        }
    }
}
