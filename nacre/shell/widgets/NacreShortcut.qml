import QtQuick
import qs.services

QtObject {
    id: root
    property string appid: "nacre_shell"
    property string name: ""
    property string description: ""
    property string triggerDescription: ""
    readonly property var params: ({
            appid: appid,
            name: name,
            description: description,
            triggerDescription: triggerDescription
        })
    property var registration: null
    readonly property bool held: registration?.held === true
    signal pressed
    signal released
    onParamsChanged: if (registration)
        registration.params = params
    Component.onCompleted: registration = NacreCompositor.createShortcut(root, params)
    property Connections events: Connections {
        target: root.registration
        function onPressed() {
            root.pressed();
        }
        function onReleased() {
            root.released();
        }
    }
}
