import QtQuick
import Quickshell.Hyprland as Native

QtObject {
    id: root
    required property var params
    property var nativeShortcut: null
    property bool ready: false
    property bool alive: true
    property bool replacing: false
    property bool held: false
    signal pressed
    signal released
    onParamsChanged: if (ready)
        replace()
    Component.onCompleted: {
        ready = true;
        nativeShortcut = factory.createObject(root, metadata());
    }
    Component.onDestruction: alive = false
    function metadata() {
        const value = params || {};
        return {
            appid: String(value.appid ?? "nacre_shell"),
            name: String(value.name ?? ""),
            description: String(value.description ?? ""),
            triggerDescription: String(value.triggerDescription ?? "")
        };
    }
    function replace() {
        if (replacing)
            return;
        replacing = true;
        if (held) {
            held = false;
            released();
        }
        if (nativeShortcut) {
            nativeShortcut.destroy();
            nativeShortcut = null;
        } else {
            retired();
        }
    }
    function retired() {
        if (!alive || !replacing)
            return;
        // Schedule from actual destruction, after native unregister completes.
        // Scheduling immediately after destroy() could recreate the same key
        // before deferred deletion releases the old registration.
        Qt.callLater(() => {
            if (Qt.isQtObject(root) && root.alive) {
                root.replacing = false;
                root.nativeShortcut = factory.createObject(root, root.metadata());
            }
        });
    }
    property Component factory: Component {
        Native.GlobalShortcut {
            onPressed: if (!root.replacing && root.alive) {
                root.held = true;
                root.pressed();
            }
            onReleased: if (!root.replacing && root.alive) {
                root.held = false;
                root.released();
            }
            Component.onDestruction: if (Qt.isQtObject(root))
                root.retired()
        }
    }
}
