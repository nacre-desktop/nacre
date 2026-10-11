import QtQuick
import qs.widgets
import qs.services

NacreSettingsPage {
    property string page: "desktop"
    NacreSettingsSection {
        title: "Windows and spacing"
        Repeater {
            model: [
                {
                    key: "gapsIn",
                    label: "Space between windows",
                    max: 30
                },
                {
                    key: "gapsOut",
                    label: "Space from screen edges",
                    max: 80
                },
                {
                    key: "borderSize",
                    label: "Window border width",
                    max: 8
                },
                {
                    key: "rounding",
                    label: "Window corner radius",
                    max: 40
                }
            ]
            Row {
                required property var modelData
                width: parent.width
                spacing: 12
                NacreText {
                    width: Math.max(0, parent.width - 192)
                    text: parent.modelData.label
                    wrapMode: Text.Wrap
                    font.pointSize: 11
                    anchors.verticalCenter: parent.verticalCenter
                }
                NacreNumberField {
                    objectName: "desktopNumber" + parent.modelData.key
                    label: parent.modelData.label
                    from: 0
                    to: parent.modelData.max
                    value: DesktopSettings.data[parent.modelData.key] ?? 0
                    onAdjusted: number => DesktopSettings.set(parent.modelData.key, number)
                }
            }
        }
    }
    NacreSettingsSection {
        title: "Effects and input"
        Repeater {
            model: [
                {
                    key: "animations",
                    label: "Animations"
                },
                {
                    key: "blur",
                    label: "Background blur"
                },
                {
                    key: "shadow",
                    label: "Window shadows"
                },
                {
                    key: "followMouse",
                    label: "Focus follows pointer"
                },
                {
                    key: "naturalScroll",
                    label: "Natural touchpad scrolling"
                }
            ]
            NacreSettingToggle {
                required property var modelData
                label: modelData.label
                setting: modelData.key
            }
        }
    }
    NacreSettingsSection {
        title: "Edge menu activation"
        description: "Three visible frame lips mark the menus without moving windows. Hover opens only those targets; status icons use clicks."
        Flow {
            width: parent.width
            spacing: 8
            ActionButton {
                text: "Click only"
                selected: DesktopSettings.data.clickEdgeMenus === true
                onClicked: DesktopSettings.set("clickEdgeMenus", true)
            }
            ActionButton {
                text: "Hover lips"
                selected: DesktopSettings.data.clickEdgeMenus !== true
                onClicked: DesktopSettings.set("clickEdgeMenus", false)
            }
        }
    }
    NacreText {
        width: parent.width
        text: DesktopSettings.message || ""
        font.pointSize: 10
        color: NacreTokens.mutedInk
        wrapMode: Text.Wrap
    }
}
