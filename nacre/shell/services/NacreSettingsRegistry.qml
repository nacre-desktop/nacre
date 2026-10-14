pragma Singleton
import QtQuick
import Quickshell
import qs.modules.settings.prototype

Singleton {
    readonly property var schema: data.schema
    function text(id, args) {
        return data.text(id, args);
    }
    SettingsRegistry {
        id: data
    }
}
