import QtQuick

Loader {
    property bool loading: active
    property bool activeAsync: false
    onActiveAsyncChanged: active = activeAsync
}
