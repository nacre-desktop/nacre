pragma Singleton
import QtQuick

QtObject {
    property bool onBattery: true
    property var displayDevice: ({
            ready: true,
            isLaptopBattery: true,
            isPresent: true,
            state: 2,
            percentage: 1,
            timeToEmpty: 0,
            timeToFull: 0
        })
}
