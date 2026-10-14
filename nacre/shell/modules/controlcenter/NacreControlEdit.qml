import QtQuick
import QtQuick.Layouts
import qs.widgets
import qs.services

Column {
    id: root
    required property var screen
    property string section: "all"
    property bool showHeadings: true
    property bool showHint: true
    property bool showReset: true
    property bool settingsRows: false
    readonly property var selectedIds: NacreControlPreferences.tiles.filter(id => catalog.supported(id))
    readonly property var detectedEntries: catalog.entries.filter(entry => ["mic-volume", "keyboard"].includes(entry.id) && catalog.supported(entry.id) && !NacreControlPreferences.tiles.includes(entry.id))
    readonly property var availableEntries: catalog.entries.filter(entry => !["mic-volume", "keyboard"].includes(entry.id) && catalog.supported(entry.id) && !NacreControlPreferences.tiles.includes(entry.id))
    spacing: settingsRows ? NacreTokens.space.sm : 12
    function includesSection(name) {
        return section === "all" || section === name;
    }
    function focusChoice(index) {
        const row = chosen.itemAt(Math.max(0, Math.min(chosen.count - 1, index)));
        if (row && row.visible && row.enabled)
            row.removeButton.focusControl();
        else if (reset.visible && reset.enabled)
            reset.focusControl();
        else
            (detected.itemAt(0) || available.itemAt(0))?.focusControl();
    }
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
            root.focusChoice(index + direction);
        });
    }
    function add(id) {
        root.forceActiveFocus(Qt.OtherFocusReason);
        NacreControlPreferences.toggle(id);
        Qt.callLater(() => root.focusChoice(root.selectedIds.indexOf(id)));
    }
    NacreText {
        objectName: "controlEditHint"
        width: parent.width
        visible: root.showHint
        text: NacreI18n.tr("controlcenter.edit.hint", "Add, remove, and reorder your controls. Unsupported features stay hidden.")
        wrapMode: Text.Wrap
        color: NacreTokens.mutedInk
        font.pointSize: 11
    }
    NacreText {
        objectName: "controlEditDetectedHeading"
        width: parent.width
        visible: root.showHeadings && root.includesSection("detected") && root.detectedEntries.length > 0
        text: NacreI18n.tr("controlcenter.edit.detected-sliders", "Detected sliders")
        font.pointSize: 12
        font.weight: Font.Medium
    }
    GridLayout {
        width: parent.width
        visible: root.includesSection("detected") && root.detectedEntries.length > 0
        columns: root.settingsRows ? 1 : 2
        columnSpacing: 8
        Repeater {
            id: detected
            model: root.includesSection("detected") ? root.detectedEntries : []
            delegate: NacreControlAddOption {
                required property var modelData
                objectName: "controlAdd-" + modelData.id
                Layout.fillWidth: true
                Layout.minimumWidth: 0
                Layout.columnSpan: root.settingsRows ? 1 : root.detectedEntries.length === 1 ? 2 : 1
                entry: modelData
                settingsRows: root.settingsRows
                detail: NacreI18n.tr("controlcenter.edit.add-slider", "Add slider")
                onClicked: root.add(modelData.id)
            }
        }
    }
    RowLayout {
        width: parent.width
        visible: root.includesSection("layout") && (root.showHeadings || root.showReset)
        NacreText {
            objectName: "controlEditLayoutHeading"
            Layout.fillWidth: true
            visible: root.showHeadings
            text: NacreI18n.tr("controlcenter.edit.layout", "Your layout")
            font.pointSize: 12
            font.weight: Font.Medium
        }
        NacreControlButton {
            id: reset
            objectName: "controlLayoutReset"
            visible: root.showReset
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
        model: root.includesSection("layout") ? root.selectedIds : []
        delegate: RowLayout {
            id: row
            required property string modelData
            required property int index
            readonly property var entry: catalog.find(modelData)
            property alias removeButton: remove
            width: root.width
            spacing: root.settingsRows ? NacreTokens.space.sm : 4
            opacity: enabled ? 1 : .45
            implicitHeight: Math.max(remove.implicitHeight, name.implicitHeight, root.settingsRows ? NacreTokens.space.huge + 2 * NacreTokens.space.sm : 0)
            NacreIcon {
                text: row.entry.icon
                font.pointSize: root.settingsRows ? NacreTokens.iconSize.medium : 17
                color: NacreTokens.accent
            }
            NacreText {
                id: name
                Layout.fillWidth: true
                Layout.minimumWidth: 0
                text: row.entry.name
                font.pointSize: root.settingsRows ? NacreTokens.textSize.body : 11
                wrapMode: Text.Wrap
            }
            NacreControlButton {
                objectName: "controlMoveUp-" + row.modelData
                icon: "arrow_upward"
                enabled: row.index > 0
                opacity: enabled || !root.settingsRows ? 1 : .35
                label: NacreI18n.tr("controlcenter.edit.move-up", "Move {name} up", {
                    name: row.entry.name
                })
                onClicked: root.change(row.modelData, -1)
            }
            NacreControlButton {
                objectName: "controlMoveDown-" + row.modelData
                icon: "arrow_downward"
                enabled: row.index < root.selectedIds.length - 1
                opacity: enabled || !root.settingsRows ? 1 : .35
                label: NacreI18n.tr("controlcenter.edit.move-down", "Move {name} down", {
                    name: row.entry.name
                })
                onClicked: root.change(row.modelData, 1)
            }
            NacreControlButton {
                id: remove
                objectName: "controlRemove-" + row.modelData
                icon: "remove"
                opacity: enabled || !root.settingsRows ? 1 : .35
                label: NacreI18n.tr("controlcenter.edit.remove", "Remove {name}", {
                    name: row.entry.name
                })
                onClicked: root.change(row.modelData, 0)
            }
        }
    }
    NacreText {
        objectName: "controlEditAvailableHeading"
        width: parent.width
        visible: root.showHeadings && root.includesSection("available") && root.availableEntries.length > 0
        text: NacreI18n.tr("controlcenter.edit.available", "Available controls")
        font.pointSize: 12
        font.weight: Font.Medium
    }
    GridLayout {
        width: parent.width
        visible: root.includesSection("available") && root.availableEntries.length > 0
        columns: root.settingsRows ? 1 : 2
        columnSpacing: 8
        rowSpacing: 8
        Repeater {
            id: available
            model: root.includesSection("available") ? root.availableEntries : []
            delegate: NacreControlAddOption {
                required property var modelData
                objectName: "controlAdd-" + modelData.id
                Layout.fillWidth: true
                Layout.minimumWidth: 0
                entry: modelData
                settingsRows: root.settingsRows
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
