import QtQuick
import QtQuick.Layouts
import qs.widgets
import qs.services

Column {
    id: root
    required property var screen
    readonly property var selectedIds: NacreControlPreferences.tiles.filter(id => catalog.supported(id))
    readonly property var detectedEntries: catalog.entries.filter(entry => ["mic-volume", "keyboard"].includes(entry.id) && catalog.supported(entry.id) && !NacreControlPreferences.tiles.includes(entry.id))
    readonly property var availableEntries: catalog.entries.filter(entry => !["mic-volume", "keyboard"].includes(entry.id) && catalog.supported(entry.id) && !NacreControlPreferences.tiles.includes(entry.id))
    spacing: 12
    NacreControlCatalog {
        id: catalog
        screen: root.screen
    }
    function change(id, direction) {
        root.forceActiveFocus(Qt.OtherFocusReason);
        const index = selectedIds.indexOf(id);
        if (direction === 0)
            NacreControlPreferences.toggle(id);
        else
            NacreControlPreferences.move(id, direction, selectedIds);
        Qt.callLater(() => {
            const row = chosen.itemAt(Math.max(0, Math.min(selectedIds.length - 1, index + direction)));
            if (row)
                row.removeButton.focusControl();
            else
                reset.focusControl();
        });
    }
    function add(id) {
        root.forceActiveFocus(Qt.OtherFocusReason);
        NacreControlPreferences.toggle(id);
        Qt.callLater(() => chosen.itemAt(root.selectedIds.indexOf(id))?.removeButton.focusControl());
    }
    NacreText {
        width: parent.width
        text: NacreI18n.tr("controlcenter.edit.hint", "Add, remove, and reorder your controls. Unsupported features stay hidden.")
        wrapMode: Text.Wrap
        color: NacreTokens.mutedInk
        font.pointSize: 11
    }
    NacreText {
        width: parent.width
        visible: root.detectedEntries.length > 0
        text: NacreI18n.tr("controlcenter.edit.detected-sliders", "Detected sliders")
        font.pointSize: 12
        font.weight: Font.Medium
    }
    GridLayout {
        width: parent.width
        visible: root.detectedEntries.length > 0
        columns: 2
        columnSpacing: 8
        Repeater {
            model: root.detectedEntries
            delegate: NacreControlTile {
                required property var modelData
                objectName: "controlAdd-" + modelData.id
                Layout.fillWidth: true
                Layout.minimumWidth: 0
                Layout.columnSpan: root.detectedEntries.length === 1 ? 2 : 1
                label: modelData.name
                icon: modelData.icon
                detail: NacreI18n.tr("controlcenter.edit.add-slider", "Add slider")
                onClicked: root.add(modelData.id)
            }
        }
    }
    RowLayout {
        width: parent.width
        NacreText {
            Layout.fillWidth: true
            text: NacreI18n.tr("controlcenter.edit.layout", "Your layout")
            font.pointSize: 12
            font.weight: Font.Medium
        }
        NacreControlButton {
            id: reset
            objectName: "controlLayoutReset"
            label: NacreI18n.tr("controlcenter.edit.reset", "Reset")
            icon: "restart_alt"
            onClicked: {
                root.forceActiveFocus(Qt.OtherFocusReason);
                NacreControlPreferences.reset();
                Qt.callLater(() => reset.focusControl());
            }
        }
    }
    Repeater {
        id: chosen
        model: root.selectedIds
        delegate: RowLayout {
            id: row
            required property string modelData
            required property int index
            readonly property var entry: catalog.find(modelData)
            property alias removeButton: remove
            width: root.width
            spacing: 4
            NacreIcon {
                text: row.entry.icon
                font.pointSize: 17
                color: NacreTokens.accent
            }
            NacreText {
                Layout.fillWidth: true
                Layout.minimumWidth: 0
                text: row.entry.name
                font.pointSize: 11
                wrapMode: Text.Wrap
            }
            NacreControlButton {
                objectName: "controlMoveUp-" + row.modelData
                icon: "arrow_upward"
                enabled: row.index > 0
                label: NacreI18n.tr("controlcenter.edit.move-up", "Move {name} up", {
                    name: row.entry.name
                })
                onClicked: root.change(row.modelData, -1)
            }
            NacreControlButton {
                objectName: "controlMoveDown-" + row.modelData
                icon: "arrow_downward"
                enabled: row.index < root.selectedIds.length - 1
                label: NacreI18n.tr("controlcenter.edit.move-down", "Move {name} down", {
                    name: row.entry.name
                })
                onClicked: root.change(row.modelData, 1)
            }
            NacreControlButton {
                id: remove
                objectName: "controlRemove-" + row.modelData
                icon: "remove"
                label: NacreI18n.tr("controlcenter.edit.remove", "Remove {name}", {
                    name: row.entry.name
                })
                onClicked: root.change(row.modelData, 0)
            }
        }
    }
    NacreText {
        width: parent.width
        visible: root.availableEntries.length > 0
        text: NacreI18n.tr("controlcenter.edit.available", "Available controls")
        font.pointSize: 12
        font.weight: Font.Medium
    }
    GridLayout {
        width: parent.width
        columns: 2
        columnSpacing: 8
        rowSpacing: 8
        Repeater {
            model: root.availableEntries
            delegate: NacreControlTile {
                required property var modelData
                objectName: "controlAdd-" + modelData.id
                Layout.fillWidth: true
                Layout.minimumWidth: 0
                label: modelData.name
                icon: modelData.icon
                detail: NacreI18n.tr("controlcenter.edit.add", "Add to controls")
                onClicked: root.add(modelData.id)
            }
        }
    }
    NacreText {
        width: parent.width
        visible: text !== ""
        text: NacreControlPreferences.error
        wrapMode: Text.Wrap
        color: NacreColours.palette.m3error
        font.pointSize: 10
    }
}
