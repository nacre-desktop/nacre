pragma Singleton
import QtQuick

QtObject {
    property int dragThreshold: 30
    property QtObject sizes: QtObject {
        property int button: 80
    }
}
