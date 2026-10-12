pragma Singleton
import QtQuick
import "."

QtObject {
    property var edge: SidebarChat
    property var recent: []
    function load(key) {
        SidebarChat.load(key);
    }
    function select(model) {
    }
}
