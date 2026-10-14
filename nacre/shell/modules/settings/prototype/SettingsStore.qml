import QtQuick
import "../registry/registry.js" as Registry

// Owning providers publish snapshots and receive validated writes. This API has
// no file paths, Python port, hardware actions or polling of its own.
QtObject {
    id: root
    property var schema: ({
            settings: []
        })
    property bool development: false
    property var capabilities: ({})
    property var files: ({})
    property var profileValues: ({})
    property var availableStores: []
    property string error: ""
    signal writeRequested(var setting, var value)
    signal resetRequested(var settings)
    signal actionRequested(string action)
    signal confirmationRequested(var setting, var value)
    function issue(page) {
        return error;
    }
    function choiceOptions(setting) {
        return [];
    }
    function value(setting) {
        if (Object.prototype.hasOwnProperty.call(profileValues, setting.id))
            return profileValues[setting.id];
        return Registry.readPath(files[setting.store.id], setting.store.key, setting.default);
    }
    function displayValue(setting, text) {
        const current = value(setting);
        if (setting.type === "boolean")
            return text(current ? "settings.registry.ui.enabled" : "settings.registry.ui.disabled");
        if (setting.type === "enum")
            return text(setting.options.find(option => option.value === current)?.labelId);
        if (setting.type === "array")
            return text("settings.registry.ui.items").replace("{count}", (current || []).length);
        if (current === "")
            return text("settings.registry.ui.default");
        return String(current) + (setting.unit ? " " + setting.unit : "");
    }
    function provenance(setting) {
        return Object.prototype.hasOwnProperty.call(profileValues, setting.id) ? "profile" : "";
    }
    function available(setting) {
        return setting.backend !== "planned" && availableStores.includes(setting.store.id);
    }
    function enabled(setting) {
        const parent = schema.settings.find(item => item.id === setting.dependsOn);
        return !setting.editorOwned && available(setting) && provenance(setting) !== "profile" && Registry.allowed(setting.enabledWhen, capabilities) && (!parent || value(parent) === true);
    }
    function write(setting, next, confirmed = false) {
        // QML models wrap data objects; ownership is by ID, not JS identity.
        // Always dispatch the canonical descriptor, never caller-supplied paths.
        const owned = schema.settings.find(item => item.id === setting?.id);
        if (!owned || !enabled(owned) || !Registry.settingsFor(schema, owned.page, capabilities, development).some(item => item.id === owned.id) || !Registry.validValue(owned, next) || owned.type === "enum" && !Registry.allowed(owned.options.find(option => option.value === next)?.enabledWhen, capabilities))
            return false;
        if (owned.apply === "confirm" && !confirmed) {
            confirmationRequested(owned, next);
            return false;
        }
        if (JSON.stringify(value(owned)) !== JSON.stringify(next))
            writeRequested(owned, next);
        return true;
    }
    function accept(file, document) {
        files = Object.assign({}, files, {
            [file]: document
        });
    }
    function reset(page) {
        const settings = Registry.settingsFor(schema, page, capabilities, development).filter(item => available(item) && provenance(item) !== "profile" && Registry.allowed(item.enabledWhen, capabilities));
        resetRequested(settings);
    }
}
