pragma Singleton
import QtQuick

QtObject {
    property var toplevels: ({
            values: []
        })
    property var workspaces: ({
            values: []
        })
    property var monitors: ({
            values: []
        })
    property var focusedMonitor: null
    property var focusedWorkspace: null
    property var activeToplevel: null
    property bool usingLua: true
    property var requests: []
    property int refreshes: 0
    signal rawEvent(var event)
    function dispatch(value) {
        requests = [...requests, value];
    }
    function refreshToplevels() {
        refreshes++;
    }
    function refreshMonitors() {
    }
    function refreshWorkspaces() {
    }
}
