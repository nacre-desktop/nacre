pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

Singleton {
    id: root
    property bool secondsEnabled: false
    property alias enabled: clock.enabled
    readonly property date date: clock.date
    readonly property int hours: clock.hours
    readonly property int minutes: clock.minutes
    readonly property int seconds: clock.seconds
    function format(pattern) {
        return Qt.formatDateTime(date, pattern);
    }
    SystemClock {
        id: clock
        objectName: "nacreSystemClock"
        precision: root.secondsEnabled ? SystemClock.Seconds : SystemClock.Minutes
    }
    IpcHandler {
        target: "clock"
        function state(): string {
            return JSON.stringify({
                enabled: root.enabled,
                secondsEnabled: root.secondsEnabled,
                local: root.format("yyyy-MM-dd HH:mm"),
                hours: root.hours,
                minutes: root.minutes
            });
        }
    }
}
