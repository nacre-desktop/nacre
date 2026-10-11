import QtQuick
import QtQuick.Controls
import qs.services
import qs.widgets

SearchSurface {
    id: root

    readonly property var matches: DesktopExtras.keys.filter(k => {
        return (k.key + " " + k.description).toLowerCase().includes(query.toLowerCase());
    })

    title: "Keyboard shortcuts"
    placeholder: "Search a shortcut or action"
    Component.onCompleted: DesktopExtras.request("keys", {})
    onMoved: delta => {
        return list.contentY = Math.max(0, Math.min(list.contentHeight - list.height, list.contentY + delta * 47));
    }

    ListView {
        id: list

        anchors.fill: parent
        clip: true
        spacing: 5
        model: root.matches

        FastScroll {
            view: list
        }

        NacreText {
            anchors.centerIn: parent
            visible: list.count === 0
            text: DesktopExtras.busy.keys ? "Reading shortcuts…" : "No matching shortcuts"
        }

        ScrollBar.vertical: ScrollBar {}

        delegate: NacreSurface {
            required property var modelData

            width: list.width
            height: 42
            radius: 10
            color: NacreColours.palette.m3surfaceContainer

            NacreText {
                anchors.left: parent.left
                anchors.leftMargin: 12
                anchors.verticalCenter: parent.verticalCenter
                width: 340
                text: modelData.key
                font.family: "JetBrains Mono NF"
                font.pointSize: 11
                color: NacreColours.palette.m3primary
            }

            NacreText {
                anchors.left: parent.left
                anchors.leftMargin: 365
                anchors.right: parent.right
                anchors.rightMargin: 12
                anchors.verticalCenter: parent.verticalCenter
                text: modelData.description
                elide: Text.ElideRight
                font.pointSize: 11
            }
        }
    }
}
