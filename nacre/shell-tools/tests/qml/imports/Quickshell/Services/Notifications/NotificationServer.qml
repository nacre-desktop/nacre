import QtQuick

QtObject {
    property bool keepOnReload: false
    property bool actionsSupported: false
    property bool imageSupported: false
    property bool persistenceSupported: false
    property bool bodyMarkupSupported: false
    signal notification(var notification)
}
