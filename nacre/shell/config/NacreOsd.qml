pragma Singleton
import QtQuick

QtObject {
    property int hideDelay: 2000
    property QtObject sizes: QtObject {
        property int sliderWidth: 30
        property int sliderHeight: 150
    }
}
