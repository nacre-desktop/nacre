import QtQuick

Item {
    id: root
    default property alias contents: body.data
    implicitHeight: body.implicitHeight
    implicitWidth: 880
    Column {
        id: body
        width: parent.width
        spacing: 16
    }
}
