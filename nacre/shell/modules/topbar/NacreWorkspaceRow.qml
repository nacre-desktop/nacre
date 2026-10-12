import QtQuick
import QtQuick.Controls.Basic
import qs.widgets
import qs.services
import qs.config

Row {
    id: root
    spacing: 3
    readonly property bool customIcons: NacreBar.workspaceIcons.some(icon => !!icon)
    readonly property var workspaceIds: {
        const ids = new Set(Array.from({
            length: NacreBar.workspaceCount
        }, (_, index) => index + 1));
        for (const id of [NacreHyprland.activeWsId, ...NacreHyprland.clients.map(client => client.workspace?.id)]) {
            if (Number.isInteger(id) && id >= 1)
                ids.add(id);
        }
        return [...ids].sort((a, b) => a - b);
    }
    function activate(id) {
        if (Number.isInteger(id) && workspaceIds.includes(id))
            return NacreHyprland.dispatch("workspace " + id);
        return false;
    }
    Repeater {
        model: root.workspaceIds
        delegate: NacreSurface {
            id: pill
            required property int index
            required property int modelData
            readonly property int workspaceId: modelData
            readonly property string accentRole: NacrePresentation.active.workspaceColors ? ["primary", "secondary", "tertiary"][(workspaceId - 1) % 3] : "primary"
            readonly property color accent: NacreColours.palette["m3" + accentRole]
            readonly property color accentInk: NacreColours.palette["m3on" + accentRole.charAt(0).toUpperCase() + accentRole.slice(1)]
            readonly property bool selected: NacreHyprland.activeWsId === workspaceId
            readonly property int windowCount: NacreHyprland.clients.filter(client => client.workspace?.id === workspaceId).length
            readonly property bool occupied: windowCount > 0
            objectName: "nacreWorkspace" + workspaceId
            implicitWidth: root.customIcons ? 46 : 30
            implicitHeight: 30
            radius: 15
            color: root.customIcons && selected ? accent : pointer.hovered ? Qt.alpha(accent, .12) : "transparent"
            HoverHandler {
                id: pointer
            }
            Rectangle {
                objectName: "workspacePearl"
                anchors.centerIn: parent
                visible: !root.customIcons
                width: pill.selected ? 17 : pill.occupied ? 12 : 8
                height: width
                radius: width / 2
                antialiasing: true
                border.width: pill.selected ? 1 : 0
                border.color: Qt.alpha(NacreColours.palette.m3onSurface, .55)
                gradient: Gradient {
                    GradientStop {
                        position: 0
                        color: pill.selected ? Qt.tint(pill.accent, Qt.alpha("#fff9f1", .75)) : pill.occupied ? Qt.tint(pill.accent, Qt.alpha("#fff9f1", .40)) : NacreColours.palette.m3outline
                    }
                    GradientStop {
                        position: .45
                        color: pill.selected ? Qt.tint(Qt.tint(pill.accent, Qt.alpha(NacreColours.palette.m3tertiary, .25)), Qt.alpha("#fff9f1", NacreColours.light ? .48 : .12)) : pill.occupied ? pill.accent : Qt.alpha(NacreColours.palette.m3outline, .75)
                    }
                    GradientStop {
                        position: 1
                        color: pill.selected ? Qt.tint(Qt.tint(pill.accent, Qt.alpha(NacreColours.palette.m3secondary, .40)), Qt.alpha("#fff9f1", NacreColours.light ? .55 : .18)) : pill.occupied ? Qt.tint(pill.accent, Qt.alpha(NacreColours.palette.m3surface, .30)) : Qt.alpha(NacreColours.palette.m3outline, .50)
                    }
                }
            }
            Row {
                anchors.centerIn: parent
                visible: root.customIcons
                spacing: 4
                NacreIcon {
                    visible: !!NacreBar.workspaceIcons[pill.workspaceId - 1]
                    text: NacreBar.workspaceIcons[pill.workspaceId - 1] || ""
                    font.pointSize: 11
                    color: pill.selected ? pill.accentInk : NacreColours.palette.m3onSurfaceVariant
                }
                NacreText {
                    text: pill.workspaceId
                    font.pointSize: 10
                    color: pill.selected ? pill.accentInk : NacreColours.palette.m3onSurfaceVariant
                }
            }
            Rectangle {
                width: 3
                height: 3
                radius: 2
                anchors.horizontalCenter: parent.horizontalCenter
                anchors.bottom: parent.bottom
                anchors.bottomMargin: 2
                visible: root.customIcons && pill.occupied
                color: pill.selected ? pill.accentInk : pill.accent
            }
            NacreInteraction {
                accessibleName: "Workspace " + pill.workspaceId + (NacreBar.workspaceNames[pill.workspaceId - 1] ? ": " + NacreBar.workspaceNames[pill.workspaceId - 1] : "") + (pill.selected ? ", current" : "") + ", " + pill.windowCount + (pill.windowCount === 1 ? " window" : " windows")
                function onClicked() {
                    root.activate(pill.workspaceId);
                }
            }
            ToolTip.visible: pointer.hovered
            ToolTip.delay: 450
            ToolTip.text: "Workspace " + pill.workspaceId + (pill.selected ? " · Current" : "") + " · " + pill.windowCount + (pill.windowCount === 1 ? " window" : " windows")
        }
    }
}
