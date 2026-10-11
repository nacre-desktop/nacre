import QtQuick
import QtQuick.Controls
import Quickshell
import Quickshell.Io
import qs.services
import qs.widgets

SearchSurface {
    id: root

    readonly property var matches: DesktopExtras.clips.filter(c => {
        return c.title.toLowerCase().includes(query.toLowerCase());
    })

    function copy(index) {
        const item = matches[index];
        if (item) {
            DesktopExtras.request("copy", {
                "id": item.id
            });
            visibilities.launcher = false;
        }
    }

    title: "Clipboard"
    placeholder: "Search copied text or images"
    Component.onCompleted: DesktopExtras.request("clips", {})
    onChosen: copy(list.currentIndex)
    onQueryChanged: list.currentIndex = 0
    onMoved: delta => {
        return list.currentIndex = Math.max(0, Math.min(matches.length - 1, list.currentIndex + delta));
    }

    Connections {
        function onLauncherChanged() {
            if (root.visibilities.launcher && root.visibilities.launcherMode === "clipboard")
                DesktopExtras.request("clips", {});
        }

        target: root.visibilities
    }

    Process {
        running: root.visibilities.launcher && root.visibilities.launcherMode === "clipboard"
        command: ["inotifywait", "-q", "-m", "-e", "close_write,moved_to", "--format", "%f", Quickshell.env("HOME") + "/.cache/cliphist"]

        stdout: SplitParser {
            onRead: name => {
                if (name === "db")
                    DesktopExtras.request("clips", {});
            }
        }
    }

    ListView {
        id: list

        anchors.fill: parent
        clip: true
        spacing: 8
        currentIndex: 0
        model: root.matches

        FastScroll {
            view: list
        }

        NacreText {
            anchors.centerIn: parent
            visible: list.count === 0
            text: DesktopExtras.busy.clips ? "Reading clipboard…" : "Nothing here yet"
            color: NacreColours.palette.m3onSurfaceVariant
        }

        ScrollBar.vertical: ScrollBar {}

        delegate: NacreSurface {
            id: tile

            required property var modelData
            required property int index

            width: list.width
            height: modelData.image ? 112 : 72
            radius: 15
            color: ListView.isCurrentItem ? NacreColours.palette.m3secondaryContainer : NacreColours.palette.m3surfaceContainer

            Image {
                id: thumbnail

                visible: tile.modelData.thumbnail.length > 0
                source: visible ? "file://" + tile.modelData.thumbnail : ""
                sourceSize.width: 130
                sourceSize.height: 90
                fillMode: Image.PreserveAspectFit
                x: 12
                y: 10
                width: 130
                height: 90
                asynchronous: true
            }

            NacreText {
                x: tile.modelData.image ? 154 : 14
                y: 14
                width: list.width - x - 178
                height: parent.height - 22
                wrapMode: Text.Wrap
                maximumLineCount: 3
                elide: Text.ElideRight
                text: tile.modelData.title
                textFormat: Text.PlainText
                font.pointSize: 11
            }

            MouseArea {
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onEntered: list.currentIndex = tile.index
                onClicked: root.copy(tile.index)
            }

            Row {
                anchors.right: parent.right
                anchors.rightMargin: 12
                anchors.verticalCenter: parent.verticalCenter
                spacing: 6

                ActionButton {
                    text: tile.modelData.pinned ? "Unpin" : "Pin"
                    enabled: !DesktopExtras.busy.pin
                    selected: tile.modelData.pinned
                    onClicked: DesktopExtras.request("pin", {
                        "id": tile.modelData.id
                    })
                }

                ActionButton {
                    text: ""
                    icon: "delete"
                    enabled: !DesktopExtras.busy.delete
                    onClicked: DesktopExtras.request("delete", {
                        "id": tile.modelData.id
                    })
                }
            }
        }
    }
}
