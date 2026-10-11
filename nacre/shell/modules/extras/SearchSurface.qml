import qs.widgets
import qs.services
import QtQuick
import Quickshell
import Quickshell.Io

Item {
    id: root
    required property PersistentProperties visibilities
    property string title
    property string placeholder: "Search"
    property int bodyHeight: 450
    property alias query: search.text
    default property alias contents: body.data
    signal moved(int delta)
    signal chosen
    implicitWidth: 860
    implicitHeight: bodyHeight + 112
    IpcHandler {
        target: "featureSearch"
        function query(text: string): void {
            root.query = text;
        }
        function state(): string {
            return JSON.stringify({
                title: root.title,
                query: root.query
            });
        }
    }
    Row {
        id: heading
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.margins: 16
        height: 30
        spacing: 12
        NacreText {
            width: parent.width - 110
            text: root.title
            font.pointSize: 17
            color: NacreColours.palette.m3primary
            anchors.verticalCenter: parent.verticalCenter
        }
        ActionButton {
            text: "Close"
            icon: "close"
            onClicked: root.visibilities.launcher = false
        }
    }
    NacreTextField {
        id: search
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: heading.bottom
        anchors.margins: 16
        height: 44
        leftPadding: 15
        rightPadding: 15
        placeholderText: root.placeholder
        background: NacreSurface {
            color: NacreColours.palette.m3surfaceContainerHigh
            radius: 22
        }
        Keys.onDownPressed: root.moved(1)
        Keys.onUpPressed: root.moved(-1)
        Keys.onReturnPressed: root.chosen()
        Keys.onEnterPressed: root.chosen()
        Keys.onEscapePressed: root.visibilities.launcher = false
        Component.onCompleted: forceActiveFocus()
        Connections {
            target: root.visibilities
            function onLauncherRequestChanged() {
                search.forceActiveFocus();
            }
        }
    }
    Item {
        id: body
        anchors.top: search.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.margins: 16
    }
}
