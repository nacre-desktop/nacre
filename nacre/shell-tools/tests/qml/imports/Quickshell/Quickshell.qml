pragma Singleton
import QtQuick

QtObject {
    property var environment: ({
            HOME: "/fixture",
            USER: "fixture-user",
            NACRE_LANGUAGE: "en"
        })
    property var screens: [
        {
            name: "test",
            width: 1920,
            height: 1080
        }
    ]
    property int processId: 0
    property var launched: []
    property var files: ({})
    function env(name) {
        return environment[name] || "";
    }
    function execDetached(command) {
        launched = [...launched, command];
    }
    function iconPath(name) {
        return "";
    }
    function reload(hard) {
    }
}
