import QtQuick
import qs.widgets

Rectangle {
    id: root
    property var entry: ({})
    property string mode: "dark"
    function role(key, fallback) {
        const value = entry.themeModes?.[mode]?.[key] || fallback;
        return /^[a-fA-F0-9]{6}$/.test(value) ? "#" + value : value;
    }
    implicitHeight: width * 0.62
    color: role("background", mode === "dark" ? "#141414" : "#f4f4f4")
    radius: 10
    border.width: 1
    border.color: role("outlineVariant", "#555555")
    Rectangle {
        x: 9
        y: 9
        width: parent.width - 18
        height: 17
        radius: 5
        color: root.role("surfaceContainerHigh", "#303030")
        Row {
            x: 7
            anchors.verticalCenter: parent.verticalCenter
            spacing: 4
            Repeater {
                model: root.entry.colors || []
                Rectangle {
                    required property string modelData
                    width: 5
                    height: 5
                    radius: 2.5
                    color: /^[a-fA-F0-9]{6}$/.test(modelData) ? "#" + modelData : modelData
                }
            }
        }
    }
    Rectangle {
        x: 9
        y: 33
        width: parent.width * 0.27
        height: parent.height - 42
        radius: 6
        color: root.role("surfaceContainer", "#252525")
        Rectangle {
            x: 6
            y: 8
            width: parent.width - 12
            height: 11
            radius: 3
            color: root.role("primaryContainer", root.role("primary", "#888888"))
        }
        Column {
            x: 7
            y: 27
            spacing: 9
            Repeater {
                model: 3
                Rectangle {
                    width: root.width * 0.15
                    height: 3
                    radius: 1.5
                    color: root.role("outlineVariant", "#555555")
                }
            }
        }
    }
    Rectangle {
        x: parent.width * 0.27 + 16
        y: 33
        width: parent.width - x - 9
        height: parent.height - 42
        radius: 6
        color: root.role("surface", "#202020")
        Column {
            x: 9
            y: 10
            width: parent.width - 18
            spacing: 8
            Rectangle {
                width: parent.width * 0.6
                height: 6
                radius: 2
                color: root.role("onSurface", "#dddddd")
            }
            Repeater {
                model: 2
                Rectangle {
                    width: parent.width
                    height: 3
                    radius: 1.5
                    color: root.role("onSurfaceVariant", "#999999")
                }
            }
            Rectangle {
                width: parent.width * 0.48
                height: 13
                radius: 4
                color: root.role("primary", "#aaaaaa")
            }
        }
    }
}
