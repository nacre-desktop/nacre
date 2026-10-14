pragma ComponentBehavior: Bound
import QtQuick
import qs.widgets
import qs.services
import qs.modules.topbar as Bar

NacreClip {
    id: root
    required property var registry
    required property var store
    color: NacreTokens.frame
    radius: NacreTokens.radii.medium
    implicitHeight: NacreTokens.space.huge * 8
    Image {
        anchors.fill: parent
        source: NacreWallpapers.thumbnail ? "file://" + NacreWallpapers.thumbnail : ""
        fillMode: Image.PreserveAspectCrop
        asynchronous: true
    }
    Rectangle {
        width: parent.width
        height: NacreTokens.space.huge + NacreTokens.space.sm
        color: NacreTokens.body
        Row {
            x: NacreTokens.space.md
            anchors.verticalCenter: parent.verticalCenter
            spacing: NacreTokens.space.sm
            BrandLogo {
                width: NacreTokens.space.xxl
                height: width
                motionEnabled: false
            }
            Repeater {
                model: 6
                Bar.NacreWorkspacePearl {
                    required property int index
                    workspaceId: index + 1
                    occupied: index < 3
                    current: index === 0
                    accent: [NacreTokens.accent, NacreColours.palette.m3secondary, NacreColours.palette.m3tertiary][index % 3]
                    anchors.verticalCenter: parent.verticalCenter
                }
            }
        }
        Row {
            anchors.right: parent.right
            anchors.rightMargin: NacreTokens.space.md
            anchors.verticalCenter: parent.verticalCenter
            spacing: NacreTokens.space.sm
            NacreIcon {
                text: "volume_up"
                font.pointSize: NacreTokens.iconSize.small
            }
            NacreIcon {
                text: "wifi"
                font.pointSize: NacreTokens.iconSize.small
            }
            NacreText {
                text: "12:30"
                font.pointSize: NacreTokens.textSize.caption
            }
        }
    }
    NacreSurface {
        x: NacreTokens.space.md
        y: NacreTokens.space.huge * 2
        width: parent.width - x * 2
        height: NacreTokens.space.huge * 3
        radius: NacreTokens.radii.medium
        color: NacreTokens.raised
        NacreText {
            x: NacreTokens.space.md
            y: NacreTokens.space.sm
            text: root.registry.text("settings.registry.panels.label")
            font.weight: Font.Medium
        }
        Row {
            x: NacreTokens.space.md
            y: NacreTokens.space.huge + NacreTokens.space.md
            spacing: NacreTokens.space.sm
            Repeater {
                model: [
                    {
                        label: root.registry.text("settings.registry.ui.primary-action"),
                        selected: true
                    },
                    {
                        label: root.registry.text("settings.registry.ui.cancel"),
                        selected: false
                    }
                ]
                NacreSurface {
                    required property var modelData
                    width: label.implicitWidth + NacreTokens.space.xl * 2
                    height: NacreTokens.space.huge
                    radius: NacreTokens.radii.small
                    color: modelData.selected ? NacreTokens.accent : NacreTokens.body
                    NacreText {
                        id: label
                        anchors.centerIn: parent
                        text: parent.modelData.label
                        color: parent.modelData.selected ? NacreTokens.focusInk(NacreTokens.accent) : NacreTokens.ink
                        font.pointSize: NacreTokens.textSize.caption
                    }
                }
            }
        }
    }
    Row {
        x: NacreTokens.space.md
        y: NacreTokens.space.huge * 5 + NacreTokens.space.md
        spacing: NacreTokens.space.md
        Repeater {
            model: [
                {
                    role: "m3success",
                    text: NacreI18n.tr("colors.theme-preview.text.ready", "✓ Ready")
                },
                {
                    role: "yellow",
                    text: NacreI18n.tr("colors.theme-preview.text.warning", "! Warning")
                },
                {
                    role: "m3error",
                    text: NacreI18n.tr("settings.registry.ui.preview-error", "Error")
                }
            ]
            NacreSurface {
                required property var modelData
                width: (root.width - NacreTokens.space.md * 4) / 3
                height: NacreTokens.space.huge + NacreTokens.space.sm
                radius: NacreTokens.radii.small
                color: NacreTokens.body
                NacreText {
                    anchors.centerIn: parent
                    text: parent.modelData.text
                    color: NacreColours.palette[parent.modelData.role] || NacreTokens.accent
                    font.pointSize: NacreTokens.textSize.caption
                }
            }
        }
    }
}
