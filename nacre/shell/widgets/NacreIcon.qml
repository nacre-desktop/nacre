import QtQuick

NacreText {
    property real fill: 0
    font.family: "Material Symbols Rounded"
    font.pointSize: 15
    font.variableAxes: ({
            "FILL": Math.max(0, Math.min(1, fill))
        })
    antialiasing: true
    Accessible.ignored: true
}
