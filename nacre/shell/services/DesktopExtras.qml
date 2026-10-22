pragma Singleton
import Quickshell
import Quickshell.Io
import QtQuick
import qs.utils

Singleton {
    id: root
    property var chats: []
    property var clips: []
    property var keys: []
    property var notes: []
    property string brainQuery: ""
    property string notesQuery: ""
    property string message: ""
    property var busy: ({})
    property var reloads: ({})
    property string captured: ""
    function request(action, payload) {
        if (busy[action]) {
            if (["clips", "keys", "chats"].includes(action)) {
                const pending = Object.assign({}, reloads);
                pending[action] = true;
                reloads = pending;
            }
            return;
        }
        const b = Object.assign({}, busy);
        b[action] = true;
        busy = b;
        job.createObject(root, {
            action: action,
            payload: payload ?? {}
        });
    }
    function accept(action, result) {
        if (result.error) {
            message = result.error;
            return;
        }
        message = "";
        if (action === "chats")
            chats = result.result;
        if (action === "clips")
            clips = result.result;
        if (action === "keys")
            keys = result.result;
        if (action === "brain" && result.query === brainQuery) {
            notes = result.result;
            notesQuery = result.query;
        }
        if (action === "pin" || action === "delete")
            request("clips", {});
        if (action === "capture")
            captured = NacreI18n.tr("ui.desktop-extras.captured.saved-to-your-brain", "Saved to your brain");
    }
    Component {
        id: job
        Process {
            id: worker
            required property string action
            required property var payload
            stdinEnabled: true
            command: [NacrePaths.tools + "/nacre-helper", "desktop-extras", action]
            Component.onCompleted: running = true
            onStarted: {
                write(JSON.stringify(payload));
                stdinEnabled = false;
            }
            stdout: SplitParser {
                splitMarker: ""
                onRead: line => {
                    try {
                        root.accept(worker.action, JSON.parse(line));
                    } catch (e) {
                        root.message = NacreI18n.tr("ui.desktop-extras.message.could-not-read-desktop-data", "Could not read desktop data");
                    }
                }
            }
            onExited: {
                const b = Object.assign({}, root.busy);
                b[action] = false;
                root.busy = b;
                if (root.reloads[action]) {
                    const pending = Object.assign({}, root.reloads);
                    delete pending[action];
                    root.reloads = pending;
                    root.request(action, {});
                }
                if (action === "brain" && payload.query !== root.brainQuery)
                    root.request("brain", {
                        query: root.brainQuery
                    });
                destroy();
            }
        }
    }
    IpcHandler {
        target: "extras"
        function counts(): string {
            return JSON.stringify({
                chats: root.chats.length,
                clips: root.clips.length,
                keys: root.keys.length,
                notes: root.notes.length,
                busy: root.busy,
                message: root.message
            });
        }
        function refresh(kind: string): void {
            root.request(kind, {});
        }
        function search(query: string): void {
            root.brainQuery = query;
            root.request("brain", {
                query: query
            });
        }
    }
}
