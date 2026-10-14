import QtQuick
import QtQuick.Controls.Basic
import Quickshell.Io
import qs.widgets
import qs.services

Column {
    id: root
    required property string address
    property string deviceName: ""
    property string prompt: ""
    property string mode: "waiting"
    property string buffer: ""
    property bool requested: false
    property bool finished: false
    signal paired
    signal cancelled
    spacing: 12
    function acceptOutput(chunk) {
        if (finished)
            return;
        buffer = (buffer + chunk.replace(/\x1b\[[0-?]*[ -/]*[@-~]/g, "")).slice(-2048);
        if (!requested && buffer.includes("Agent registered")) {
            requested = true;
            buffer = "";
            agent.write("pair " + address + "\n");
            return;
        }
        if (/Failed to pair|AuthenticationFailed|AuthenticationCanceled|not available|No default controller|Failed to register agent/.test(buffer)) {
            mode = "error";
            prompt = NacreI18n.tr("controlcenter.bluetooth.pair-failed", "Could not pair. Make the device discoverable and try again.");
            buffer = "";
            agent.signal(15);
            finished = true;
            return;
        }
        if (buffer.includes("Pairing successful")) {
            finished = true;
            buffer = "";
            agent.write("exit\n");
            paired();
            return;
        }
        const confirmation = buffer.match(/Confirm passkey\s+(\d+)/);
        const display = buffer.match(/(?:Passkey|PIN code):\s*(\d+)/);
        if (confirmation) {
            mode = "confirm";
            prompt = NacreI18n.tr("controlcenter.bluetooth.confirm-code", "Does {code} match the code on your device?", {
                code: confirmation[1]
            });
        } else if (/Enter PIN code|Enter passkey|Request PIN code|Request passkey/.test(buffer)) {
            mode = "pin";
            prompt = NacreI18n.tr("controlcenter.bluetooth.enter-code", "Enter the pairing code from your device.");
        } else if (display) {
            mode = "display";
            prompt = NacreI18n.tr("controlcenter.bluetooth.type-code", "Type {code} on your device, then press Enter.", {
                code: display[1]
            });
        } else if (/Authorize service|Confirm authorization/.test(buffer)) {
            mode = "confirm";
            prompt = NacreI18n.tr("controlcenter.bluetooth.authorize", "Allow this device to connect?");
        }
    }
    function respond(value) {
        if (!agent.running || finished || !["pin", "confirm"].includes(mode) || /[\r\n]/.test(value) || !value)
            return;
        agent.write(value + "\n");
        code.clear();
        buffer = "";
        mode = "waiting";
        prompt = "";
    }
    function cancel() {
        finished = true;
        code.clear();
        if (agent.running)
            agent.signal(15);
        cancelled();
    }
    NacreText {
        width: parent.width
        text: NacreI18n.tr("controlcenter.bluetooth.pair-device", "Pair with {name}", {
            name: root.deviceName
        })
        font.pointSize: 12
        font.weight: Font.Medium
        elide: Text.ElideRight
    }
    NacreText {
        width: parent.width
        text: root.prompt || NacreI18n.tr("controlcenter.bluetooth.confirm-device", "Confirm pairing on your device.")
        wrapMode: Text.Wrap
        color: root.mode === "error" ? NacreColours.palette.m3error : NacreTokens.mutedInk
        font.pointSize: 10
        Accessible.role: Accessible.StaticText
        Accessible.name: text
    }
    TextField {
        id: code
        objectName: "controlBluetoothCode"
        visible: root.mode === "pin"
        width: parent.width
        maximumLength: 16
        echoMode: TextInput.Password
        placeholderText: NacreI18n.tr("controlcenter.bluetooth.code", "Pairing code")
        Accessible.name: placeholderText
        color: NacreTokens.ink
        font.family: NacreTokens.textFamily
        placeholderTextColor: NacreTokens.mutedInk
        onVisibleChanged: if (visible)
            Qt.callLater(() => forceActiveFocus(Qt.OtherFocusReason))
        onAccepted: root.respond(text)
        background: Rectangle {
            radius: 12
            color: NacreTokens.body
            border.width: 1
            border.color: code.activeFocus ? NacreTokens.accent : NacreTokens.outline
        }
    }
    Row {
        spacing: 8
        ActionButton {
            visible: root.mode === "confirm" || root.mode === "pin"
            text: root.mode === "confirm" ? NacreI18n.tr("controlcenter.bluetooth.confirm", "Confirm") : NacreI18n.tr("controlcenter.bluetooth.pair", "Pair")
            selected: true
            onClicked: root.respond(root.mode === "confirm" ? "yes" : code.text)
        }
        ActionButton {
            text: NacreI18n.tr("controlcenter.network.cancel", "Cancel")
            onClicked: root.cancel()
        }
    }
    Process {
        id: agent
        objectName: "controlBluetoothAgent"
        command: ["bluetoothctl", "--agent", "KeyboardDisplay"]
        stdinEnabled: true
        running: /^[0-9A-Fa-f]{2}(:[0-9A-Fa-f]{2}){5}$/.test(root.address)
        stdout: SplitParser {
            splitMarker: ""
            onRead: chunk => root.acceptOutput(chunk)
        }
        stderr: SplitParser {
            splitMarker: ""
            onRead: chunk => root.acceptOutput(chunk)
        }
        onExited: (exitCode, exitStatus) => {
            if (!root.finished) {
                root.mode = "error";
                root.prompt = NacreI18n.tr("controlcenter.bluetooth.pair-failed", "Could not pair. Make the device discoverable and try again.");
                root.finished = true;
            }
        }
    }
    Timer {
        interval: 60000
        running: agent.running
        onTriggered: {
            root.mode = "error";
            root.prompt = NacreI18n.tr("controlcenter.bluetooth.pair-timeout", "Pairing timed out. Make the device discoverable and try again.");
            root.finished = true;
            code.clear();
            agent.signal(15);
        }
    }
    Component.onDestruction: if (agent.running)
        agent.signal(15)
}
