import QtQuick

Loader {
    property Component component
    sourceComponent: component
    property bool loading: active
    property bool activeAsync: false
    onActiveAsyncChanged: active = activeAsync
}
