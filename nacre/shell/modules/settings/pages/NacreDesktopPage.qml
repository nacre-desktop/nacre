import QtQuick
import qs.widgets
import qs.services

NacreSettingsPage {
    property string page: "desktop"
    NacreText {
        width: parent.width
        visible: NacreCompositor.capabilities?.perOutputWorkspaces === true
        text: NacreI18n.tr("ui.compositor.niri-settings-limits", "Niri uses one window gap. Outer spacing is configured in niri. Live previews, sticky windows, and shaped panel blur are unavailable.")
        wrapMode: Text.Wrap
        color: NacreTokens.mutedInk
    }
    NacreSettingsSection {
        title: NacreI18n.tr("ui.desktop-page.title.windows-and-spacing", "Windows and spacing")
        Repeater {
            model: [
                {
                    key: "gapsIn",
                    label: NacreI18n.tr("ui.desktop-page.label.space-between-windows", "Space between windows"),
                    max: 30
                },
                {
                    key: "gapsOut",
                    label: NacreI18n.tr("ui.desktop-page.label.space-from-screen-edges", "Space from screen edges"),
                    max: 80
                },
                {
                    key: "borderSize",
                    label: NacreI18n.tr("ui.desktop-page.label.window-border-width", "Window border width"),
                    max: 8
                },
                {
                    key: "rounding",
                    label: NacreI18n.tr("ui.desktop-page.label.window-corner-radius", "Window corner radius"),
                    max: 40
                }
            ]
            Row {
                required property var modelData
                width: parent.width
                spacing: NacreTokens.space.md
                NacreText {
                    width: Math.max(0, parent.width - 192)
                    text: parent.modelData.label
                    wrapMode: Text.Wrap
                    font.pointSize: NacreTokens.textSize.body
                    anchors.verticalCenter: parent.verticalCenter
                }
                NacreNumberField {
                    enabled: !NacreCompositor.capabilities?.desktopOptions || NacreCompositor.capabilities.desktopOptions.includes(parent.modelData.key)
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
        title: NacreI18n.tr("ui.desktop-page.title.effects-and-input", "Effects and input")
        Repeater {
            model: [
                {
                    key: "animations",
                    label: NacreI18n.tr("ui.desktop-page.label.animations", "Animations")
                },
                {
                    key: "blur",
                    label: NacreI18n.tr("ui.desktop-page.label.background-blur", "Background blur")
                },
                {
                    key: "shadow",
                    label: NacreI18n.tr("ui.desktop-page.label.window-shadows", "Window shadows")
                },
                {
                    key: "followMouse",
                    label: NacreI18n.tr("ui.desktop-page.label.focus-follows-pointer", "Focus follows pointer")
                },
                {
                    key: "naturalScroll",
                    label: NacreI18n.tr("ui.desktop-page.label.natural-touchpad-scrolling", "Natural touchpad scrolling")
                }
            ]
            NacreSettingToggle {
                required property var modelData
                enabled: !NacreCompositor.capabilities?.desktopOptions || NacreCompositor.capabilities.desktopOptions.includes(modelData.key)
                label: modelData.label
                setting: modelData.key
            }
        }
    }
    NacreSettingsSection {
        title: NacreI18n.tr("ui.desktop-page.title.edge-menu-activation", "Edge menu activation")
        description: NacreI18n.tr("ui.desktop-page.description.three-visible-frame-lips-mark-the-menus-without-moving", "Three visible frame lips mark the menus without moving windows. Hover opens only those targets; status icons use clicks.")
        Flow {
            width: parent.width
            spacing: NacreTokens.space.sm
            ActionButton {
                text: NacreI18n.tr("ui.desktop-page.text.click-only", "Click only")
                selected: DesktopSettings.data.clickEdgeMenus === true
                onClicked: DesktopSettings.set("clickEdgeMenus", true)
            }
            ActionButton {
                text: NacreI18n.tr("ui.desktop-page.text.hover-lips", "Hover lips")
                selected: DesktopSettings.data.clickEdgeMenus !== true
                onClicked: DesktopSettings.set("clickEdgeMenus", false)
            }
        }
    }
    NacreSettingsSection {
        title: NacreI18n.tr("ui.desktop-page.title.panel-windows", "Panel windows")
        description: NacreI18n.tr("ui.desktop-page.description.open-any-edge-panel-as-a-movable-resizable-window-drag", "Open any edge panel as a movable, resizable window. Drag its pearl grip away from the edge; use Dock to return it. Launcher windows stay open after launching apps.")
        NacreSettingToggle {
            label: NacreI18n.tr("ui.desktop-page.label.allow-detachable-panels", "Allow detachable panels")
            setting: "detachablePanels"
        }
        NacreSettingToggle {
            label: NacreI18n.tr("ui.desktop-page.label.drag-pearl-grips-to-detach-and-dock", "Drag pearl grips to detach and dock")
            setting: "dragPanels"
        }
    }
    NacreText {
        width: parent.width
        text: DesktopSettings.message || ""
        font.pointSize: NacreTokens.textSize.caption
        color: NacreTokens.mutedInk
        wrapMode: Text.Wrap
    }
}
