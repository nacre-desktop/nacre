pragma Singleton
import QtQuick

// Used only by the isolated preview's synthetic service adapters.
QtObject {
    property var store: null
    property var backend: null
}
