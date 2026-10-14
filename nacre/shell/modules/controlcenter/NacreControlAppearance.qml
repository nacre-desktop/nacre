import QtQuick
import QtQuick.Layouts
import qs.widgets
import qs.services

NacreSurface {
    id: root
    readonly property string mode: NacreWallpapers.preferences.paletteMode || (NacreTokens.light ? "light" : "dark")
    implicitHeight: 44
    radius: 22
    color: NacreTokens.raised
    RowLayout {
        anchors.fill: parent
        anchors.margins: 4
        spacing: 2
        Repeater {
            model: [
                {
                    id: "light",
                    label: NacreI18n.tr("controlcenter.overview.light", "Light"),
                    icon: "light_mode"
                },
                {
                    id: "dark",
                    label: NacreI18n.tr("controlcenter.overview.dark", "Dark"),
                    icon: "dark_mode"
                },
                {
                    id: "auto",
                    label: NacreI18n.tr("controlcenter.appearance.auto", "Auto"),
                    icon: "brightness_auto"
                }
            ]
            delegate: NacreSurface {
                required property var modelData
                readonly property bool selected: root.mode === modelData.id
                Layout.fillWidth: true
                Layout.fillHeight: true
                radius: 18
                color: selected ? NacreColours.palette.m3primaryContainer : "transparent"
                Row {
                    anchors.centerIn: parent
                    spacing: 6
                    NacreIcon {
                        text: modelData.icon
                        font.pointSize: 13
                        color: parent.parent.selected ? NacreColours.palette.m3onPrimaryContainer : NacreTokens.mutedInk
                    }
                    NacreText {
                        text: modelData.label
                        font.pointSize: 10
                        color: parent.parent.selected ? NacreColours.palette.m3onPrimaryContainer : NacreTokens.ink
                    }
                }
                NacreInteraction {
                    accessibleName: modelData.label
                    function onClicked() {
                        NacreColours.setMode(modelData.id);
                    }
                }
            }
        }
    }
}
