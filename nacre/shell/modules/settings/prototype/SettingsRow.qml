pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import qs.widgets
import "../registry/registry.js" as Registry

NacreSurface {
    id: root
    required property var setting
    required property var registry
    required property var store
    signal entitiesRequested
    property bool highlighted: false
    readonly property var currentValue: store.value(setting)
    readonly property bool stacked: width < NacreTokens.space.huge * 17
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
        NacreSurface {
            visible: root.profile
            color: NacreTokens.body
            radius: NacreTokens.radii.small
            width: badgeText.implicitWidth + NacreTokens.space.md * 2
            height: badgeText.implicitHeight + NacreTokens.space.xs * 2
            NacreText {
                id: badgeText
                anchors.centerIn: parent
                text: root.registry.text("settings.registry.ui.profile")
                font.pointSize: NacreTokens.textSize.caption
                color: NacreTokens.mutedInk
            }
        }
        NacreText {
            width: parent.width
            visible: !root.profile && (!root.store.available(root.setting) || !Registry.allowed(root.setting.enabledWhen, root.store.capabilities))
            text: root.registry.text(root.setting.unavailableId || "settings.registry.ui.unavailable")
            wrapMode: Text.Wrap
            color: NacreTokens.mutedInk
            font.pointSize: NacreTokens.textSize.caption
            font.weight: Font.Medium
        }
    }
    Loader {
        id: editor
        x: parent.width - width - NacreTokens.space.sm
        y: root.stacked ? labels.y + labels.height + NacreTokens.space.sm : Math.max(NacreTokens.space.md, (labels.height - height) / 2 + labels.y)
        width: root.setting.type === "boolean" ? NacreTokens.space.xl * 3 : Math.min(root.stacked ? parent.width - NacreTokens.space.md * 2 : NacreTokens.space.huge * 8 + NacreTokens.space.sm * 2, parent.width - NacreTokens.space.md * 2)
        enabled: root.editable
        opacity: enabled ? 1 : .45
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
                implicitWidth: NacreTokens.space.xxl + NacreTokens.space.xl + NacreTokens.space.xs
                implicitHeight: NacreTokens.space.xl + NacreTokens.space.sm
                x: parent.leftPadding
                y: (parent.height - height) / 2
                radius: NacreTokens.radii.full
                color: root.currentValue === true ? NacreTokens.accent : NacreTokens.body
                border.width: 1
                border.color: editor.item?.visualFocus ? NacreTokens.accent : NacreTokens.outline
                NacreSurface {
                    x: root.currentValue === true ? parent.width - width - NacreTokens.space.xs : NacreTokens.space.xs
                    y: NacreTokens.space.xs
                    width: NacreTokens.radii.large
                    height: NacreTokens.radii.large
                    radius: NacreTokens.radii.full
                    color: root.currentValue === true ? NacreTokens.focusInk(NacreTokens.accent) : NacreTokens.mutedInk
                }
            }
        }
    }
    FontMetrics {
        id: segmentMetrics
        font.family: NacreTokens.textFamily
        font.pointSize: NacreTokens.textSize.caption
    }
    Component {
        id: segments
        Item {
            implicitHeight: buttons.implicitHeight
            Flow {
                id: buttons
                x: parent.width - width
                width: Math.min(parent.width, root.setting.options.reduce((sum, option) => sum + Math.ceil(segmentMetrics.advanceWidth(root.registry.text(option.labelId))) + NacreTokens.space.xl + NacreTokens.space.hairline, 0) + Math.max(0, root.setting.options.length - 1) * NacreTokens.space.xs)
                spacing: NacreTokens.space.xs
                Repeater {
                    model: root.setting.options
                    ActionButton {
                        required property var modelData
                        objectName: "option-" + root.setting.id + "-" + modelData.value
                        text: root.registry.text(modelData.labelId)
                        compact: true
                        enabled: Registry.allowed(modelData.enabledWhen, root.store.capabilities)
                        opacity: root.editable && !enabled ? .45 : 1
                        selected: modelData.value === root.currentValue
                        onClicked: root.store.write(root.setting, modelData.value)
                    }
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
            palette.window: NacreTokens.body
            palette.base: NacreTokens.raised
            palette.text: NacreTokens.ink
            palette.button: NacreTokens.raised
            palette.buttonText: NacreTokens.ink
            palette.highlight: NacreTokens.accent
            palette.highlightedText: NacreTokens.focusInk(NacreTokens.accent)
            background: NacreSurface {
                radius: NacreTokens.radii.small
                color: NacreTokens.body
                border.width: 1
                border.color: editor.item?.visualFocus ? NacreTokens.accent : NacreTokens.outline
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
                palette.window: NacreTokens.body
                palette.base: NacreTokens.raised
                palette.text: NacreTokens.ink
                palette.button: NacreTokens.raised
                palette.buttonText: NacreTokens.ink
                palette.highlight: NacreTokens.accent
                palette.highlightedText: NacreTokens.focusInk(NacreTokens.accent)
            }
        }
    }
    Component {
        id: entry
        NacreTextField {
            objectName: "control-" + root.setting.id
            text: String(root.currentValue)
            placeholderText: root.registry.text("settings.registry.ui.default")
            accessibleName: root.registry.text(root.setting.labelId)
            onEditingFinished: root.store.write(root.setting, text)
        }
    }
    Component {
        id: list
        ActionButton {
            objectName: "control-" + root.setting.id
            text: root.registry.text("settings.registry.ui.items", {
                count: (root.currentValue || []).length
            })
            icon: "chevron_right"
            compact: true
            onClicked: root.entitiesRequested()
        }
    }
}
