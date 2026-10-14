import QtQuick
import Quickshell.Io
import qs.services
import "../registry/registry.js" as Registry

Item {
    id: root
    required property var store
    required property string path
    property bool ready: false
    property bool writable: false
    property var pendingFiles: null
    function save(files) {
        if (!ready || !writable)
            return false;
        pendingFiles = files;
        file.setText(JSON.stringify({
            version: 1,
            files: files
        }) + "\n");
        return true;
    }
    function change(filename, keys, value) {
        const files = Object.assign({}, pendingFiles || store.files);
        files[filename] = Registry.writePath(files[filename], keys, value);
        return save(files);
    }
    FileView {
        id: file
        objectName: "settingsPreviewFile"
        path: root.path
        blockLoading: true
        watchChanges: true
        atomicWrites: true
        printErrors: false
        onFileChanged: reload()
        onLoaded: {
            try {
                const value = JSON.parse(text());
                if (value.version !== 1 || !value.files || Array.isArray(value.files) || typeof value.files !== "object")
                    throw new Error();
                root.store.files = value.files;
                root.store.error = "";
                root.ready = true;
                root.writable = true;
            } catch (error) {
                root.writable = false;
                root.store.error = NacreI18n.tr("settings.preview.state-invalid", "Preview state is invalid or newer; original file preserved.");
            }
        }
        onSaved: {
            if (root.pendingFiles)
                root.store.files = root.pendingFiles;
            root.pendingFiles = null;
        }
        onSaveFailed: {
            root.pendingFiles = null;
            root.store.error = NacreI18n.tr("settings.preview.save-failed", "Preview state could not be saved.");
        }
    }
    Connections {
        target: root.store
        function onWriteRequested(setting, value) {
            root.change(setting.store.file, setting.store.key, value);
        }
        function onResetRequested(settings) {
            const files = Object.assign({}, root.store.files);
            for (const setting of settings)
                files[setting.store.file] = Registry.writePath(files[setting.store.file], setting.store.key, setting.default);
            root.save(files);
        }
        function onActionRequested(action) {
            if (action === "launcher-history") {
                const files = Object.assign({}, root.store.files);
                files["launcher.json"] = Object.assign({}, files["launcher.json"], {
                    recent: [],
                    usage: {}
                });
                root.save(files);
            }
        }
    }
}
