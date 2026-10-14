pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

Singleton {
    id: root
    property var profiles: []
    property string error: ""
    property bool ready: false
    readonly property bool busy: reader.running || worker.running
    readonly property var connected: profiles.filter(profile => profile.active)
    function fields(line) {
        const values = [];
        let value = "", escaped = false;
        for (const character of line) {
            if (escaped) {
                value += character;
                escaped = false;
            } else if (character === "\\")
                escaped = true;
            else if (character === ":") {
                values.push(value);
                value = "";
            } else
                value += character;
        }
        values.push(value);
        return values;
    }
    function receive(text) {
        profiles = text.trim().split("\n").map(line => fields(line)).filter(row => row.length === 4 && /^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$/.test(row[0]) && ["vpn", "wireguard"].includes(row[1])).map(row => ({
                    uuid: row[0],
                    name: row[2],
                    active: row[3] !== "" && row[3] !== "--"
                }));
        ready = true;
    }
    function refresh() {
        if (!busy)
            reader.running = true;
    }
    function activate(uuid, enabled) {
        if (busy || !profiles.some(profile => profile.uuid === uuid) || typeof enabled !== "boolean")
            return false;
        error = "";
        worker.command = ["nmcli", "--wait", "30", "connection", enabled ? "up" : "down", "uuid", uuid];
        worker.running = true;
        return true;
    }
    function toggle() {
        const profile = connected[0] || profiles[0];
        return profile ? activate(profile.uuid, !profile.active) : false;
    }
    Process {
        id: reader
        objectName: "controlVpnReader"
        command: ["nmcli", "--colors", "no", "--terse", "--escape", "yes", "--fields", "UUID,TYPE,NAME,DEVICE", "connection", "show"]
        stdout: StdioCollector {
            onStreamFinished: root.receive(text)
        }
        onExited: (exitCode, exitStatus) => {
            if (exitCode !== 0 || exitStatus !== 0) {
                root.profiles = [];
                root.ready = false;
                root.error = NacreI18n.tr("controlcenter.vpn.unavailable", "NetworkManager VPN controls are unavailable.");
            }
        }
    }
    Process {
        id: worker
        objectName: "controlVpnWorker"
        onExited: (exitCode, exitStatus) => {
            root.error = exitCode === 0 && exitStatus === 0 ? "" : NacreI18n.tr("controlcenter.vpn.failed", "Could not change the VPN connection. Check its saved credentials and try again.");
            root.refresh();
        }
    }
}
