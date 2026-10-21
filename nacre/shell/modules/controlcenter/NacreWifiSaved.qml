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
        if (!enabled)
            return;
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
        if (!enabled || !outputReady || !exitReady)
            return;
        if (readingProfiles) {
            pending = exitCode === 0 ? output.trim().split("\n").map(line => line.split(":")).filter(row => row.length === 2 && ["wifi", "802-11-wireless"].includes(row[1]) && /^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$/.test(row[0])).map(row => row[0]).slice(0, 32) : [];
            readingProfiles = false;
        } else if (exitCode === 0 && output.trim())
            ssids = [...ssids, output.trim()];
        if (pending.length) {
            const uuid = pending[0];
            pending = pending.slice(1);
            nextProfile.uuid = uuid;
            nextProfile.restart();
        } else
            ready = true;
    }
    Component.onCompleted: refresh()
    onEnabledChanged: {
        if (enabled)
            refresh();
        else {
            nextProfile.stop();
            pending = [];
            reader.running = false;
        }
    }
    // The page owns the deferred query, so unloading cancels it with the reader.
    Timer {
        id: nextProfile
        property string uuid: ""
        interval: 0
        onTriggered: root.start(["nmcli", "--colors", "no", "--get-values", "802-11-wireless.ssid", "connection", "show", "uuid", uuid])
    }
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
