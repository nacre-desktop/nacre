pragma ComponentBehavior: Bound
import QtQuick
import Quickshell.Io
import qs.widgets
import qs.services
import qs.utils

Column {
    id: root
    required property var registry
    required property var store
    readonly property var published: NacreSettingsStorage.documents["theme-targets"]?.targets || []
    readonly property var targets: {
        if (published.length)
            return published;
        try {
            return JSON.parse(manifest.text()).targets || [];
        } catch (error) {
            return [];
        }
    }
    spacing: NacreTokens.space.sm
    FileView {
        id: manifest
        path: NacrePaths.tools + "/theme-targets.json"
        blockLoading: true
        printErrors: false
    }
    Repeater {
        model: root.targets
        delegate: SettingsRow {
            id: target
            required property var modelData
            readonly property var descriptor: root.registry.schema.settings.find(row => row.id === "colors/app-theming.targets." + modelData.id)
            visible: !!descriptor
            width: root.width
            registry: root.registry
            store: root.store
            setting: descriptor || ({})
            statusText: root.registry.text(modelData.status === "tool-missing" ? "settings.registry.ui.theme-missing" : root.store.value(setting) === false ? "settings.registry.colors.target.paused" : modelData.status === "current" ? "settings.registry.ui.theme-current" : "settings.registry.ui.publisher-status")
            description: {
                const canonical = descriptor?.store.key[1];
                const shared = root.targets.filter(row => {
                    const other = root.registry.schema.settings.find(item => item.id === "colors/app-theming.targets." + row.id);
                    return other?.store.key[1] === canonical && row.id !== modelData.id;
                }).map(row => root.registry.text(row.labelId));
                if (shared.length) {
                    return root.registry.text("settings.registry.colors.target.shared", {
                        apps: shared.join(", ")
                    });
                }
                return root.registry.text(
                    descriptor?.descriptionId || "settings.registry.colors.target.description"
                );
            }
        }
    }
}
