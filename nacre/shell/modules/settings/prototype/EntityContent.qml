pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import qs.widgets
import qs.services
import "../../launcher/menu-model.js" as Menu

SettingsCard {
    id: root
    required property var setting
    required property var registry
    required property var store
    signal back
    property string filter: ""
    property bool customCommand: false
    readonly property bool defaultApplication: setting.store.id === "apps" && setting.section === "apps.defaults"
    readonly property var installedApplications: defaultApplication ? NacreApps.all.filter(entry => entry.command?.length) : []
    readonly property bool navigation: setting.entityType?.startsWith("navigation-") === true
    readonly property var navigationRows: navigation ? Menu.rail(NacreMenu.rows, setting.entityType === "navigation-order" ? {
        order: store.value(setting)
    } : {}) : []
    function move(index, direction) {
        if (index < 1 || index + direction < 1 || index + direction >= navigationRows.length)
            return;
        const values = navigationRows.map(item => item.id);
        [values[index], values[index + direction]] = [values[index + direction], values[index]];
        store.write(setting, values);
    }
    title: registry.text(setting.labelId)
    description: registry.text(setting.descriptionId)
    ActionButton {
        text: root.registry.text("settings.registry.ui.back")
        icon: "arrow_back"
        compact: true
        onClicked: root.back()
    }
    NacreTextField {
        visible: root.setting.entityType === "app"
        width: parent.width
        placeholderText: root.registry.text("settings.registry.ui.apps")
        onTextEdited: root.filter = text
    }
    NacreSelect {
        objectName: "defaultApplicationPicker"
        visible: root.defaultApplication
        width: Math.min(parent.width, NacreTokens.space.huge * 12)
        model: root.installedApplications.map(entry => entry.name)
        currentIndex: root.installedApplications.findIndex(entry => JSON.stringify(NacreApps.commandFor(entry, true)) === JSON.stringify(root.store.value(root.setting)))
        displayText: currentIndex >= 0 ? currentText : root.registry.text("settings.registry.ui.choose-app")
        searchable: true
        enabled: root.store.enabled(root.setting)
        onActivated: index => root.store.write(root.setting, NacreApps.commandFor(root.installedApplications[index], true))
    }
    ActionButton {
        visible: root.defaultApplication
        text: root.registry.text("settings.registry.ui.custom-command")
        compact: true
        onClicked: root.customCommand = !root.customCommand
    }
    Repeater {
        model: root.setting.entityType === "app" ? NacreApps.all.filter(item => !root.filter || item.name.toLocaleLowerCase().includes(root.filter.toLocaleLowerCase())) : []
        delegate: Item {
            id: app
            required property var modelData
            width: parent.width
            implicitHeight: Math.max(name.implicitHeight, choice.implicitHeight) + NacreTokens.space.md * 2
            NacreText {
                id: name
                width: parent.width - choice.width - NacreTokens.space.xxl
                anchors.verticalCenter: parent.verticalCenter
                text: app.modelData.name
                wrapMode: Text.Wrap
            }
            ActionButton {
                id: choice
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                compact: true
                icon: selected ? "check" : "add"
                text: ""
                accessibleLabel: name.text
                enabled: root.store.enabled(root.setting)
                selected: root.store.value(root.setting).includes(app.modelData.id)
                onClicked: {
                    const values = root.store.value(root.setting);
                    root.store.write(root.setting, selected ? values.filter(id => id !== app.modelData.id) : [...values, app.modelData.id]);
                }
            }
        }
    }
    Repeater {
        model: root.navigation ? root.navigationRows : []
        delegate: Item {
            id: category
            required property var modelData
            required property int index
            width: parent.width
            implicitHeight: NacreTokens.space.huge + NacreTokens.space.lg
            NacreText {
                width: parent.width - buttons.width - NacreTokens.space.md
                anchors.verticalCenter: parent.verticalCenter
                text: category.modelData.label
                elide: Text.ElideRight
            }
            Row {
                id: buttons
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                spacing: NacreTokens.space.xs
                ActionButton {
                    visible: root.setting.entityType === "navigation-order"
                    icon: "arrow_upward"
                    text: ""
                    accessibleLabel: root.registry.text("settings.registry.ui.move-up")
                    compact: true
                    enabled: category.index > 1 && root.store.enabled(root.setting)
                    onClicked: root.move(category.index, -1)
                }
                ActionButton {
                    visible: root.setting.entityType === "navigation-order"
                    icon: "arrow_downward"
                    text: ""
                    accessibleLabel: root.registry.text("settings.registry.ui.move-down")
                    compact: true
                    enabled: category.index > 0 && category.index < root.navigationRows.length - 1 && root.store.enabled(root.setting)
                    onClicked: root.move(category.index, 1)
                }
                NacreToggleSwitch {
                    visible: root.setting.entityType === "navigation-hidden"
                    enabled: category.modelData.id !== "home" && root.store.enabled(root.setting)
                    checked: !root.store.value(root.setting).includes(category.modelData.id)
                    Accessible.name: category.modelData.label
                    onToggled: {
                        const values = root.store.value(root.setting);
                        root.store.write(root.setting, checked ? values.filter(id => id !== category.modelData.id) : [...values, category.modelData.id]);
                    }
                }
            }
        }
    }
    Repeater {
        model: root.setting.entityType === "app" || root.navigation || root.defaultApplication && !root.customCommand ? [] : root.store.value(root.setting)
        delegate: Row {
            id: argument
            required property string modelData
            required property int index
            width: parent.width
            spacing: NacreTokens.space.sm
            NacreTextField {
                width: parent.width - remove.width - parent.spacing
                text: argument.modelData
                accessibleName: root.registry.text(root.setting.labelId)
                enabled: root.store.enabled(root.setting)
                onEditingFinished: {
                    const values = root.store.value(root.setting).slice();
                    values[argument.index] = text;
                    root.store.write(root.setting, values);
                }
            }
            ActionButton {
                id: remove
                icon: "close"
                text: ""
                accessibleLabel: root.registry.text("settings.registry.ui.remove")
                compact: true
                enabled: root.store.enabled(root.setting) && !(root.setting.entityType === "command" && argument.index === 0)
                onClicked: root.store.write(root.setting, root.store.value(root.setting).filter((value, index) => index !== argument.index))
            }
        }
    }
    ActionButton {
        visible: root.setting.entityType !== "app" && !root.navigation && (!root.defaultApplication || root.customCommand)
        text: root.registry.text("settings.registry.ui.add")
        icon: "add"
        compact: true
        enabled: root.store.enabled(root.setting)
        onClicked: root.store.write(root.setting, [...root.store.value(root.setting), ""])
    }
}
