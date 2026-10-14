pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io
import qs.utils

Singleton {
    id: root
    readonly property var defaults: ["volume", "brightness", "network", "bluetooth", "battery", "night", "dnd", "awake", "appearance"]
    property var tiles: defaults.slice()
    readonly property var optionalTiles: tiles.filter(id => !defaults.includes(id))
    property string error: ""
    readonly property var allowed: defaults.concat(["airplane", "vpn", "record", "mic", "mic-volume", "keyboard", "screenshot", "picker", "clipboard", "wallpaper", "next-wallpaper", "updates", "notifications", "displays"])
    function normalize(values) {
        return [...new Set(values.filter(value => typeof value === "string" && allowed.includes(value)))];
    }
    function save(values) {
        tiles = normalize(values);
        error = "";
        store.saveText(JSON.stringify({
            version: 2,
            tiles: tiles
        }));
    }
    function toggle(id) {
        if (!allowed.includes(id))
            return;
        if (tiles.includes(id)) {
            save(tiles.filter(value => value !== id));
            return;
        }
        const ordered = tiles.slice(), after = id === "mic-volume" ? "volume" : id === "keyboard" ? "brightness" : "";
        const index = after ? ordered.indexOf(after) : -1;
        ordered.splice(index < 0 ? ordered.length : index + 1, 0, id);
        save(ordered);
    }
    function move(id, offset, visibleIds = tiles) {
        if (![1, -1].includes(offset) || !tiles.includes(id))
            return false;
        const visible = normalize(visibleIds).filter(value => tiles.includes(value));
        const index = visible.indexOf(id), other = visible[index + offset];
        if (index < 0 || !other)
            return false;
        const ordered = tiles.slice();
        ordered.splice(ordered.indexOf(id), 1);
        ordered.splice(ordered.indexOf(other) + (offset > 0 ? 1 : 0), 0, id);
        save(ordered);
        return true;
    }
    function reset() {
        save(defaults);
    }
    function receive(text) {
        try {
            const data = JSON.parse(text);
            if (data.version === 2 && Array.isArray(data.tiles))
                tiles = normalize(data.tiles);
            else if (data.version === 1 && Array.isArray(data.optionalTiles))
                tiles = normalize(defaults.concat(data.optionalTiles));
            else
                throw new Error();
            error = "";
            return true;
        } catch (failure) {
            error = NacreI18n.tr("controlcenter.preferences.invalid", "Could not read the saved tiles.");
            return false;
        }
    }
    NacreStateFile {
        id: store
        path: NacrePaths.config + "/control-center.json"
        printErrors: false
        atomicWrites: true
        objectName: "controlPreferencesStore"
        onLoaded: root.receive(text())
        onSaveFailed: root.error = NacreI18n.tr("controlcenter.preferences.save-failed", "Could not save the tiles.")
    }
}
