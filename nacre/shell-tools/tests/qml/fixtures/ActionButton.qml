import QtQuick

Rectangle {
    property string text: ""
    property string icon: ""
    property bool selected: false

    signal clicked

    implicitWidth: 80
    implicitHeight: 35
}
