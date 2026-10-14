import QtQuick
import Quickshell.Io

Item {
    id: root
    property var ssids: []
    property var pending: []
    property bool ready: false
    property bool readingProfiles: true
    property bool outputReady: false
    property bool exitReady: false
    property int exitCode: 0
    property string output: ""
    function start(command) {
        outputReady = false;
        exitReady = false;
        output = "";
        reader.command = command;
        reader.running = true;
    }
    function refresh() {
        if (reader.running)
            return;
        ssids = [];
        ready = false;
        readingProfiles = true;
        start(["nmcli", "--colors", "no", "--terse", "--fields", "UUID,TYPE", "connection", "show"]);
    }
    function complete() {
        if (!outputReady || !exitReady)
            return;
        if (readingProfiles) {
            pending = exitCode === 0 ? output.trim().split("\n").map(line => line.split(":")).filter(row => row.length === 2 && ["wifi", "802-11-wireless"].includes(row[1]) && /^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$/.test(row[0])).map(row => row[0]).slice(0, 32) : [];
            readingProfiles = false;
        } else if (exitCode === 0 && output.trim())
            ssids = [...ssids, output.trim()];
        if (pending.length) {
            const uuid = pending[0];
            pending = pending.slice(1);
            Qt.callLater(() => start(["nmcli", "--colors", "no", "--get-values", "802-11-wireless.ssid", "connection", "show", "uuid", uuid]));
        } else
            ready = true;
    }
    Component.onCompleted: refresh()
    Process {
        id: reader
        objectName: "controlSavedWifiReader"
        stdout: StdioCollector {
            onStreamFinished: {
                root.output = text;
                root.outputReady = true;
                root.complete();
            }
        }
        onExited: (code, status) => {
            root.exitCode = status === 0 ? code : -1;
            root.exitReady = true;
            root.complete();
        }
    }
}
