pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io
import qs.utils

Singleton {
    id: root
    property var optionalTiles: []
    property string error: ""
    readonly property var allowed: ["airplane", "vpn", "record", "mic"]
    function toggle(id) {
        if (!allowed.includes(id))
            return;
        optionalTiles = optionalTiles.includes(id) ? optionalTiles.filter(value => value !== id) : [...optionalTiles, id];
        store.setText(JSON.stringify({
            version: 1,
            optionalTiles: optionalTiles
        }));
    }
    FileView {
        id: store
        path: NacrePaths.config + "/control-center.json"
        printErrors: false
        atomicWrites: true
        onLoaded: {
            try {
                const data = JSON.parse(text());
                if (data.version === 1 && Array.isArray(data.optionalTiles))
                    root.optionalTiles = [...new Set(data.optionalTiles.filter(value => root.allowed.includes(value)))];
            } catch (failure) {
                root.error = NacreI18n.tr("controlcenter.preferences.invalid", "Could not read the saved tiles.");
            }
        }
        onSaveFailed: root.error = NacreI18n.tr("controlcenter.preferences.save-failed", "Could not save the tiles.")
    }
}
