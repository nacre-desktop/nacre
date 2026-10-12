import QtQuick

QtObject {
    id: root
    required property var owner
    required property var params
    property bool held: false
    signal pressed
    signal released
    function key() {
        return String(params?.appid || "nacre_shell") + ":" + String(params?.name || "");
    }
    function pulse() {
        if (held)
            return;
        held = true;
        pressed();
        // Niri spawn bindings carry activation, not physical release events.
        // A pulse deliberately never claims a physical-key held state.
        Qt.callLater(() => {
            if (Qt.isQtObject(root) && root.held) {
                root.held = false;
                root.released();
            }
        });
    }
    onParamsChanged: if (held) {
        held = false;
        released();
    }
    Component.onDestruction: owner.unregisterShortcut(root)
}
