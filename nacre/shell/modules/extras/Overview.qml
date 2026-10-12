import qs.widgets
import qs.services
import qs.config
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import QtQuick
import QtQuick.Controls

SearchSurface {
    id: root
    title: "Windows"
    placeholder: "Search windows or applications"
    implicitWidth: 1020
    bodyHeight: Math.min(690, Quickshell.screens[0].height - 230)
    property int selection: 0
    property int previews: 0
    property bool dragging: false
    property string draggedAddress: ""
    readonly property var windows: NacreCompositor.windows.filter(c => !c.hidden && (c.title + " " + c.wmClass).toLowerCase().includes(query.toLowerCase()))
    readonly property var workspaces: [...new Set([1, 2, 3, 4, 5, 6, 7, ...windows.map(c => c.workspace?.id).filter(id => id > 0)])].sort((a, b) => a - b)
    function focusWindow(client) {
        if (client) {
            if (!root.detached)
                visibilities.launcher = false;
            NacreCompositor.focusWindow(client.windowId);
        }
    }
    function moveWindow(address, workspace) {
        NacreCompositor.moveWindowToWorkspace(address, workspace);
    }
    onChosen: focusWindow(windows[selection])
    onMoved: delta => selection = Math.max(0, Math.min(windows.length - 1, selection + delta))
    onQueryChanged: selection = 0
    Component.onCompleted: NacreCompositor.refreshWindows()
    Flickable {
        id: viewport
        FastScroll {
            view: viewport
        }
        anchors.fill: parent
        clip: true
        contentWidth: width
        contentHeight: workspacesGrid.implicitHeight
        ScrollBar.vertical: ScrollBar {}
        Grid {
            id: workspacesGrid
            columns: 3
            spacing: 12
            Repeater {
                model: root.workspaces
                NacreSurface {
                    id: workspace
                    required property int modelData
                    width: 320
                    height: 215
                    radius: 18
                    color: drop.containsDrag ? NacreColours.palette.m3primaryContainer : NacreColours.palette.m3surfaceContainer
                    readonly property var windows: root.windows.filter(c => c.workspace?.id === modelData)
                    Row {
                        x: 12
                        y: 12
                        spacing: 8
                        NacreIcon {
                            text: NacreBar.workspaceIcons[workspace.modelData - 1] ?? "workspaces"
                            font.pointSize: 14
                            color: NacreColours.palette.m3primary
                        }
                        NacreText {
                            text: workspace.modelData + " · " + (NacreBar.workspaceNames[workspace.modelData - 1] ?? "Workspace")
                            color: NacreColours.palette.m3primary
                        }
                    }
                    MouseArea {
                        x: 0
                        y: 0
                        width: parent.width
                        height: 38
                        cursorShape: Qt.PointingHandCursor
                        onClicked: {
                            if (!root.detached)
                                visibilities.launcher = false;
                            NacreCompositor.switchWorkspace(workspace.modelData);
                        }
                    }
                    GridView {
                        id: tiles
                        x: 10
                        y: 43
                        width: 300
                        height: 160
                        cellWidth: workspace.windows.length === 1 ? 300 : 150
                        cellHeight: workspace.windows.length <= 2 ? 160 : 80
                        clip: true
                        model: workspace.windows
                        delegate: Item {
                            id: tile
                            required property var modelData
                            width: tiles.cellWidth - 5
                            height: tiles.cellHeight - 4
                            readonly property var captureHandle: modelData.captureHandle
                            NacreSurface {
                                id: preview
                                width: tile.width
                                height: tile.height - 22
                                radius: 8
                                color: root.windows[root.selection]?.windowId === tile.modelData.windowId ? NacreColours.palette.m3secondaryContainer : NacreColours.palette.m3surfaceContainerHigh
                                ScreencopyView {
                                    id: copy
                                    anchors.centerIn: parent
                                    constraintSize: Qt.size(preview.width - 8, preview.height - 8)
                                    captureSource: root.visibilities.launcher ? tile.captureHandle ?? null : null
                                    live: false
                                    onHasContentChanged: root.previews += hasContent ? 1 : -1
                                    Component.onDestruction: if (hasContent)
                                        root.previews--
                                    Timer {
                                        interval: 350
                                        repeat: true
                                        running: root.visibilities.launcher && DesktopSettings.data.livePreviews !== false
                                        onTriggered: copy.captureFrame()
                                    }
                                }
                                NacreIcon {
                                    anchors.centerIn: parent
                                    text: "window"
                                    visible: !copy.hasContent
                                    font.pointSize: 26
                                    color: NacreColours.palette.m3onSurfaceVariant
                                }
                            }
                            NacreText {
                                anchors.bottom: parent.bottom
                                width: tile.width
                                font.pointSize: 9
                                elide: Text.ElideRight
                                text: ChatWindowTitle.titles[tile.modelData.pid] || tile.modelData.title
                            }
                            Item {
                                id: dragProxy
                            }
                            MouseArea {
                                id: mouse
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                drag.target: dragProxy
                                onEntered: root.selection = root.windows.findIndex(c => c.windowId === tile.modelData.windowId)
                                onPositionChanged: event => {
                                    if (drag.active) {
                                        const p = mapToItem(root, event.x, event.y);
                                        ghost.x = p.x - 70;
                                        ghost.y = p.y - 30;
                                    }
                                }
                                onReleased: {
                                    if (root.dragging)
                                        ghost.Drag.drop();
                                    root.dragging = false;
                                    dragProxy.x = 0;
                                    dragProxy.y = 0;
                                }
                                onClicked: root.focusWindow(tile.modelData)
                                Connections {
                                    target: mouse.drag
                                    function onActiveChanged() {
                                        if (mouse.drag.active) {
                                            root.draggedAddress = tile.modelData.windowId;
                                            root.dragging = true;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    NacreText {
                        anchors.centerIn: tiles
                        visible: workspace.windows.length === 0
                        text: "Empty"
                        color: NacreColours.palette.m3onSurfaceVariant
                        font.pointSize: 11
                    }
                    DropArea {
                        id: drop
                        anchors.fill: parent
                        keys: ["nacre-window"]
                        onDropped: drop => {
                            root.moveWindow(drop.source.windowId, workspace.modelData);
                            drop.acceptProposedAction();
                        }
                    }
                }
            }
        }
    }
    NacreSurface {
        id: ghost
        parent: root
        visible: root.dragging
        z: 20
        width: 140
        height: 60
        radius: 12
        color: NacreColours.palette.m3primaryContainer
        opacity: .9
        property string windowId: root.draggedAddress
        Drag.active: root.dragging
        Drag.keys: ["nacre-window"]
        Drag.source: ghost
        Drag.hotSpot.x: 70
        Drag.hotSpot.y: 30
        NacreText {
            anchors.centerIn: parent
            text: "Move window"
            color: NacreColours.palette.m3onPrimaryContainer
        }
    }
    IpcHandler {
        target: "overview"
        function state(): string {
            return JSON.stringify({
                windows: root.windows.length,
                workspaces: root.workspaces.length,
                previews: root.previews,
                matches: root.windows.length,
                previewHandles: NacreCompositor.windows.filter(t => t.captureHandle).length
            });
        }
        function move(address: string, workspace: int): void {
            root.moveWindow(address, workspace);
        }
    }
}
