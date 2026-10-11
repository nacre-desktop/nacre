pragma Singleton
import QtQuick
import Quickshell

Singleton {
    id: root
    property bool enabled: NacreTime.enabled
    readonly property date date: NacreTime.date
    readonly property int hours: NacreTime.hours
    readonly property int minutes: NacreTime.minutes
    readonly property int seconds: NacreTime.seconds
    function format(pattern) {
        return NacreTime.format(pattern);
    }
    onEnabledChanged: if (enabled !== NacreTime.enabled)
        NacreTime.enabled = enabled
    Connections {
        target: NacreTime
        function onEnabledChanged() {
            root.enabled = NacreTime.enabled;
        }
    }
}
