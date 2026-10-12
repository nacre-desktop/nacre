import qs.widgets
import qs.services
import QtQuick
import QtQuick.Controls

SearchSurface {
    id: root
    title: "Commands"
    placeholder: "Actions, apps, windows, or brain: something"
    readonly property var entries: {
        const q = query.toLowerCase().replace(/^>\s*/, "").trim();
        if (q.startsWith("brain:"))
            return [
                {
                    name: "Search brain for " + query.slice(6).trim(),
                    description: "Open the brain drawer",
                    icon: "neurology",
                    action: "brain-query",
                    value: query.slice(6).trim()
                }
            ];
        const words = q.split(/\s+/).filter(Boolean), matches = e => words.every(w => (e.name + " " + e.description).toLowerCase().includes(w));
        const actions = DesktopActions.list.filter(item => item.launcher !== false && matches(item));
        if (!q)
            return actions;
        const windows = NacreCompositor.windows.map(c => ({
                    name: c.title,
                    description: "Window · workspace " + c.workspace?.id,
                    icon: "window",
                    window: c
                })).filter(matches);
        const apps = NacreApps.fuzzyQuery(query).slice(0, 8).map(a => ({
                    name: a.name,
                    description: "Application",
                    icon: "apps",
                    app: a
                }));
        return [...actions, ...windows, ...apps];
    }
    function activate(index) {
        const e = entries[index];
        if (!e)
            return;
        if (e.app) {
            NacreApps.launch(e.app);
            if (!root.detached)
                visibilities.launcher = false;
        } else if (e.window) {
            if (!root.detached)
                visibilities.launcher = false;
            NacreCompositor.focusWindow(e.window.windowId);
        } else if (e.action === "brain-query") {
            DesktopExtras.brainQuery = e.value;
            DesktopActions.execute("left");
            DesktopExtras.request("brain", {
                query: e.value
            });
        } else
            DesktopActions.execute(e.action, e.value);
    }
    onMoved: delta => list.currentIndex = Math.max(0, Math.min(entries.length - 1, list.currentIndex + delta))
    onChosen: activate(list.currentIndex)
    onQueryChanged: list.currentIndex = 0
    ListView {
        id: list
        anchors.fill: parent
        clip: true
        spacing: 6
        currentIndex: 0
        model: root.entries
        ScrollBar.vertical: ScrollBar {}
        FastScroll {
            view: list
        }
        delegate: NacreSurface {
            id: tile
            required property var modelData
            required property int index
            width: list.width
            height: 58
            radius: 14
            color: ListView.isCurrentItem ? NacreColours.palette.m3secondaryContainer : NacreColours.palette.m3surfaceContainer
            Row {
                anchors.fill: parent
                anchors.margins: 12
                spacing: 12
                NacreIcon {
                    text: tile.modelData.icon
                    anchors.verticalCenter: parent.verticalCenter
                    color: NacreColours.palette.m3primary
                }
                Column {
                    spacing: 2
                    NacreText {
                        width: list.width - 90
                        elide: Text.ElideRight
                        text: tile.modelData.name
                        textFormat: Text.PlainText
                    }
                    NacreText {
                        text: tile.modelData.description
                        font.pointSize: 10
                        color: NacreColours.palette.m3onSurfaceVariant
                    }
                }
            }
            MouseArea {
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onEntered: list.currentIndex = tile.index
                onClicked: root.activate(tile.index)
            }
        }
    }
}
