import QtQuick

Item {
    property var view
    property real step: 600
    property real pixelMultiplier: 3.0

    signal scrolled
    signal settled
}
