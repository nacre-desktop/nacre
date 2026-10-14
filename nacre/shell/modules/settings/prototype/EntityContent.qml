pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import qs.widgets
import qs.services

SettingsCard {
    id: root
    required property var setting
    required property var registry
    required property var store
    signal back
    property string filter: ""
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
        model: root.setting.entityType === "app" ? [] : root.store.value(root.setting)
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
        visible: root.setting.entityType !== "app"
        text: root.registry.text("settings.registry.ui.add")
        icon: "add"
        compact: true
        enabled: root.store.enabled(root.setting)
        onClicked: root.store.write(root.setting, [...root.store.value(root.setting), ""])
    }
}
