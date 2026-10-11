import QtQuick
import qs.widgets
import qs.services
import qs.config

Row {
    id: root
    spacing: 3
    function activate(id) {
        if (Number.isInteger(id) && id >= 1 && id <= NacreBar.workspaceCount)
            return NacreHyprland.dispatch("workspace " + id);
        return false;
    }
    Repeater {
        model: NacreBar.workspaceCount
        delegate: NacreSurface {
            id: pill
            required property int index
            readonly property int workspaceId: index + 1
            readonly property string accentRole: NacrePresentation.active.workspaceColors ? ["primary", "secondary", "tertiary"][index % 3] : "primary"
            readonly property color accent: NacreColours.palette["m3" + accentRole]
            readonly property color accentInk: NacreColours.palette["m3on" + accentRole.charAt(0).toUpperCase() + accentRole.slice(1)]
            readonly property bool selected: NacreHyprland.activeWsId === workspaceId
            readonly property bool occupied: NacreHyprland.clients.some(client => client.workspace?.id === workspaceId)
            objectName: "nacreWorkspace" + workspaceId
            implicitWidth: 46
            implicitHeight: 30
            radius: 15
            color: selected ? accent : "transparent"
            Row {
                anchors.centerIn: parent
                spacing: 4
                NacreIcon {
                    visible: !!NacreBar.workspaceIcons[pill.index]
                    text: NacreBar.workspaceIcons[pill.index] || ""
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
                visible: pill.occupied
                color: pill.selected ? pill.accentInk : pill.accent
            }
            NacreInteraction {
                accessibleName: "Workspace " + pill.workspaceId + ": " + (NacreBar.workspaceNames[pill.index] || "")
                function onClicked() {
                    root.activate(pill.workspaceId);
                }
            }
        }
    }
}
