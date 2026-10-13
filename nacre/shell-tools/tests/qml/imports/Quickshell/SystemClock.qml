import QtQuick

QtObject {
    enum Precision {
        Seconds,
        Minutes
    }
    property int precision: SystemClock.Minutes
    property bool enabled: true
    property date date: new Date(2026, 9, 9, 12, 34, 56)
    readonly property int hours: date.getHours()
    readonly property int minutes: date.getMinutes()
    readonly property int seconds: precision === SystemClock.Seconds ? date.getSeconds() : 0
}
