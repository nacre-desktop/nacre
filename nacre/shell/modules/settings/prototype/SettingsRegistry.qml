pragma ComponentBehavior: Bound
import QtQuick
import QtQml.Models
import Quickshell.Io
import qs.services
import "../registry/registry.js" as Registry

Item {
    id: root
    property var fragmentNames: JSON.parse(manifest.text()).fragments || []
    property int revision: 0
    property var documents: {
        const generation = revision;
        return fragmentNames.map((name, index) => fragments.objectAt(index)?.document || null);
    }
    property var messages: JSON.parse(english.text())
    readonly property var schema: Registry.merge(documents)
    function text(id, args) {
        return id ? NacreI18n.tr(id, messages[id] || id, args || {}) : "";
    }
    FileView {
        id: manifest
        path: Qt.resolvedUrl("../registry/manifest.json")
        blockLoading: true
    }
    Instantiator {
        id: fragments
        model: root.fragmentNames
        onObjectAdded: root.revision++
        onObjectRemoved: root.revision++
        delegate: FileView {
            required property string modelData
            readonly property var document: JSON.parse(text())
            path: Qt.resolvedUrl("../registry/" + modelData)
            blockLoading: true
        }
    }
    FileView {
        id: english
        path: Qt.resolvedUrl("../../../i18n/en.json")
        blockLoading: true
    }
}
