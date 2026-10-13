import QtQuick
import Quickshell
import qs.widgets
import qs.services

NacreSurface {
    id: root
    property bool detached: false
    required property PersistentProperties visibilities
    property alias query: search.text
    readonly property bool commands: query.trim().startsWith(">")
    readonly property var entries: commands ? DesktopActions.list.filter(item => item.action !== "power" && item.launcher !== false && (item.name + " " + (item.description || "")).toLowerCase().includes(query.trim().slice(1).toLowerCase())).slice(0, 8) : NacreApps.fuzzyQuery(query.trim()).slice(0, 8)
    implicitWidth: 680
    implicitHeight: 84 + Math.max(1, entries.length) * 54
    radius: 20
    color: NacreColours.palette.m3surface

    function activate(index) {
        const entry = entries[index];
        if (!entry)
            return;
        if (!root.detached)
            visibilities.launcher = false;
        if (commands)
            DesktopActions.execute(entry.action, entry.value);
        else
            NacreApps.launch(entry);
    }
    function focusSearch() {
        search.forceActiveFocus();
    }
    Component.onCompleted: {
        query = visibilities.launcherQuery || "";
        focusSearch();
    }
    Connections {
        target: root.visibilities
        function onLauncherRequestChanged() {
            root.query = root.visibilities.launcherQuery || "";
            list.currentIndex = 0;
            root.focusSearch();
        }
    }
    NacreTextField {
        id: search
        objectName: "legacyLauncherSearch"
        x: 16
        y: 16
        width: parent.width - 32
        height: 42
        padding: 12
        placeholderText: NacreI18n.tr("launcher.search-panel.placeholderText.search-apps-or-actions", "Search apps or > actions")
        onTextChanged: list.currentIndex = 0
        Keys.onDownPressed: {
            list.forceActiveFocus();
            list.currentIndex = Math.max(0, list.currentIndex);
        }
        onAccepted: root.activate(Math.max(0, list.currentIndex))
        Keys.onEscapePressed: {
            if (root.detached) {
                search.text = "";
            } else
                root.visibilities.launcher = false;
        }
    }
    ListView {
        id: list
        objectName: "legacyLauncherResults"
        x: 16
        y: 70
        width: parent.width - 32
        height: root.entries.length ? root.entries.length * 54 : 54
        model: root.entries
        clip: true
        currentIndex: 0
        boundsBehavior: Flickable.StopAtBounds
        keyNavigationEnabled: true
        Keys.onReturnPressed: root.activate(currentIndex)
        Keys.onEscapePressed: {
            if (root.detached) {
                search.text = "";
            } else
                root.visibilities.launcher = false;
        }
        Keys.onPressed: event => {
            if (event.key === Qt.Key_Backspace || event.text && !(event.modifiers & Qt.ControlModifier)) {
                if (event.key === Qt.Key_Backspace)
                    search.text = search.text.slice(0, -1);
                else
                    search.text += event.text;
                search.forceActiveFocus();
                event.accepted = true;
            }
        }
        delegate: NacreSurface {
            required property var modelData
            required property int index
            width: list.width
            height: 50
            radius: 12
            color: list.currentIndex === index ? NacreColours.palette.m3primaryContainer : "transparent"
            NacreIcon {
                x: 12
                anchors.verticalCenter: parent.verticalCenter
                text: root.commands ? parent.modelData.icon : "apps"
                color: list.currentIndex === parent.index ? NacreColours.palette.m3onPrimaryContainer : NacreColours.palette.m3onSurface
            }
            Column {
                x: 50
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width - 66
                spacing: 2
                NacreText {
                    width: parent.width
                    elide: Text.ElideRight
                    text: parent.parent.modelData.name
                    font.pointSize: 12
                    color: list.currentIndex === parent.parent.index ? NacreColours.palette.m3onPrimaryContainer : NacreColours.palette.m3onSurface
                }
                NacreText {
                    width: parent.width
                    elide: Text.ElideRight
                    text: parent.parent.modelData.description || parent.parent.modelData.comment || ""
                    font.pointSize: 9
                    color: NacreColours.palette.m3onSurfaceVariant
                }
            }
            NacreInteraction {
                function onClicked() {
                    root.activate(parent.index);
                }
            }
        }
        FastScroll {
            view: list
        }
    }
    NacreText {
        x: 24
        y: 86
        visible: !root.entries.length
        text: NacreI18n.tr("launcher.search-panel.text.no-matches", "No matches")
        color: NacreColours.palette.m3onSurfaceVariant
    }
}
