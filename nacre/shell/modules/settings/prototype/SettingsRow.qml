pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import qs.widgets

NacreSurface {
    id: root
    required property var setting
    required property var registry
    required property var store
    property bool highlighted: false
    readonly property var currentValue: store.value(setting)
    readonly property bool stacked: width < 540
    readonly property bool editable: store.enabled(setting)
    readonly property bool profile: store.provenance(setting) === "profile"
    objectName: setting.id
    radius: NacreTokens.radii.small
    color: highlighted ? Qt.alpha(NacreTokens.accent, .12) : "transparent"
    border.width: highlighted ? 1 : 0
    border.color: NacreTokens.accent
    implicitHeight: Math.max(labels.y + labels.height, editor.y + editor.height) + NacreTokens.space.md
    Column {
        id: labels
        x: NacreTokens.space.sm
        y: NacreTokens.space.md
        width: root.stacked ? parent.width - 2 * x : Math.max(0, parent.width - editor.width - NacreTokens.space.xxl)
        spacing: NacreTokens.space.xs
        NacreText {
            width: parent.width
            text: root.registry.text(root.setting.labelId)
            wrapMode: Text.Wrap
            font.pointSize: NacreTokens.textSize.body
        }
        NacreText {
            width: parent.width
            text: root.registry.text(root.setting.descriptionId)
            wrapMode: Text.Wrap
            color: NacreTokens.mutedInk
            font.pointSize: NacreTokens.textSize.caption
        }
        NacreText {
            width: parent.width
            visible: root.profile || !root.store.available(root.setting)
            text: root.profile ? root.registry.text("settings.registry.ui.profile") : root.registry.text(root.setting.unavailableId || "settings.registry.ui.unavailable")
            wrapMode: Text.Wrap
            color: NacreTokens.mutedInk
            font.pointSize: NacreTokens.textSize.caption
            font.weight: Font.Medium
        }
    }
    Loader {
        id: editor
        x: root.stacked ? NacreTokens.space.sm : parent.width - width - NacreTokens.space.sm
        y: root.stacked ? labels.y + labels.height + NacreTokens.space.sm : Math.max(NacreTokens.space.md, (labels.height - height) / 2 + labels.y)
        width: root.setting.type === "boolean" ? 54 : Math.min(root.stacked ? parent.width - NacreTokens.space.md * 2 : 272, parent.width - NacreTokens.space.md * 2)
        enabled: root.editable
        sourceComponent: root.setting.type === "boolean" ? toggle : root.setting.type === "enum" ? (root.setting.options.length <= 4 ? segments : select) : ["integer", "number"].includes(root.setting.type) ? range : root.setting.type === "array" ? list : entry
    }
    Component {
        id: toggle
        Switch {
            objectName: "control-" + root.setting.id
            checked: root.currentValue === true
            Accessible.name: root.registry.text(root.setting.labelId)
            onToggled: root.store.write(root.setting, checked)
            indicator: NacreSurface {
                implicitWidth: 46
                implicitHeight: 26
                x: parent.leftPadding
                y: (parent.height - height) / 2
                radius: NacreTokens.radii.full
                color: root.currentValue === true ? NacreTokens.accent : NacreTokens.body
                border.width: 1
                border.color: NacreTokens.outline
                NacreSurface {
                    x: root.currentValue === true ? parent.width - width - 3 : 3
                    y: 3
                    width: 20
                    height: 20
                    radius: NacreTokens.radii.full
                    color: root.currentValue === true ? NacreTokens.focusInk(NacreTokens.accent) : NacreTokens.mutedInk
                }
            }
        }
    }
    Component {
        id: segments
        Flow {
            spacing: NacreTokens.space.xs
            Repeater {
                model: root.setting.options
                ActionButton {
                    required property var modelData
                    objectName: "option-" + root.setting.id + "-" + modelData.value
                    text: root.registry.text(modelData.labelId)
                    compact: true
                    selected: modelData.value === root.currentValue
                    onClicked: root.store.write(root.setting, modelData.value)
                }
            }
        }
    }
    Component {
        id: select
        ComboBox {
            objectName: "control-" + root.setting.id
            model: root.setting.options.map(option => root.registry.text(option.labelId))
            currentIndex: root.setting.options.findIndex(option => option.value === root.currentValue)
            font.family: NacreTokens.textFamily
            font.pointSize: NacreTokens.textSize.body
            Accessible.name: root.registry.text(root.setting.labelId)
            onActivated: index => root.store.write(root.setting, root.setting.options[index].value)
            background: NacreSurface {
                radius: NacreTokens.radii.small
                color: NacreTokens.body
                border.width: 1
                border.color: NacreTokens.outline
            }
            contentItem: NacreText {
                text: root.registry.text(root.setting.options[parent.currentIndex]?.labelId)
                leftPadding: NacreTokens.space.md
                rightPadding: NacreTokens.space.huge
                verticalAlignment: Text.AlignVCenter
                elide: Text.ElideRight
            }
            indicator: NacreIcon {
                x: parent.width - width - NacreTokens.space.sm
                anchors.verticalCenter: parent.verticalCenter
                text: "expand_more"
            }
        }
    }
    Component {
        id: range
        Column {
            spacing: NacreTokens.space.xs
            NacreText {
                text: root.currentValue + " " + (root.setting.unit || "")
                font.pointSize: NacreTokens.textSize.caption
            }
            Slider {
                objectName: "control-" + root.setting.id
                width: parent.width
                from: root.setting.range.min
                to: root.setting.range.max
                stepSize: root.setting.range.step
                snapMode: Slider.SnapAlways
                value: root.currentValue
                Accessible.name: root.registry.text(root.setting.labelId)
                onMoved: root.store.write(root.setting, value)
            }
        }
    }
    Component {
        id: entry
        NacreTextField {
            objectName: "control-" + root.setting.id
            text: String(root.currentValue)
            accessibleName: root.registry.text(root.setting.labelId)
            onEditingFinished: root.store.write(root.setting, text)
        }
    }
    Component {
        id: list
        NacreText {
            text: (root.currentValue || []).join(", ")
            wrapMode: Text.Wrap
            color: NacreTokens.mutedInk
            font.pointSize: NacreTokens.textSize.caption
        }
    }
}
