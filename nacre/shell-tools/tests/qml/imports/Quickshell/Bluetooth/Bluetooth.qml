pragma Singleton
import QtQuick

QtObject {
    property var devices: ({
            values: []
        })
    property var defaultAdapter: null
}
