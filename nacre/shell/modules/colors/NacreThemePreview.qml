import QtQuick
import qs.widgets

Rectangle {
    id: root
    objectName: "desktopThemePreview"
    required property var palette
    property string wallpaper: ""
    property bool active: true
    property bool workspaceColors: false
    property bool compact: false
    width: 400
    height: 280
    readonly property bool condensed: compact || height < 260
    clip: true
    radius: 14
    color: root.c("frame")
    border.width: 1
    border.color: root.c("outline")
    function c(role) {
        return "#" + (palette?.[role] || palette?.onSurface || "e8e8e8");
    }
    NacreImage {
        x: 5
        y: root.condensed ? 30 : 35
        width: parent.width - 10
        height: parent.height - (root.condensed ? 35 : 40)
        path: root.wallpaper
        active: root.active && root.visible
        fillMode: Image.PreserveAspectCrop
        asynchronous: true
        retainWhileLoading: true
        requestedSize: Qt.size(640, 360)
    }
    BrandLogo {
        x: 10
        y: 8
        width: 20
        height: 20
        primary: root.c("primary")
        secondary: root.c("secondary")
        tertiary: root.c("tertiary")
        highlight: root.c("primaryFixed")
        background: root.c("frame")
        foreground: root.c("onSurface")
        motionEnabled: false
    }
    Row {
        x: 40
        y: 8
        spacing: 5
        Repeater {
            model: 3
            Rectangle {
                required property int index
                width: 24
                height: 20
                radius: 5
                color: index === 0 || root.workspaceColors ? root.c(["primary", "secondary", "tertiary"][index]) : "transparent"
                Text {
                    anchors.centerIn: parent
                    text: parent.index + 1
                    font.pointSize: 8
                    color: parent.index === 0 || root.workspaceColors ? root.c(["onPrimary", "onSecondary", "onTertiary"][parent.index]) : root.c("onSurface")
                }
            }
        }
    }
    Text {
        x: parent.width - 73
        y: 10
        text: "Nacre"
        font.pointSize: 9
        color: root.c("onSurface")
    }
    Rectangle {
        x: 14
        y: root.condensed ? 40 : 50
        width: parent.width - 28
        height: root.compact ? Math.min(100, root.height - 50) : root.condensed ? 80 : Math.min(160, root.height - 160)
        radius: 9
        color: root.c("surfaceContainer")
        Rectangle {
            width: parent.width
            height: 27
            radius: 9
            color: root.c("surfaceContainerHigh")
        }
        Text {
            x: 12
            y: 6
            text: "Files"
            font.pointSize: 9
            color: root.c("onSurface")
        }
        Text {
            x: 12
            y: 40
            text: root.condensed ? "Documents\nPictures" : "Documents\nPictures\nProjects"
            font.pointSize: 9
            color: root.c("onSurface")
            lineHeight: 1.3
        }
        Rectangle {
            x: parent.width - 105
            y: 44
            width: 90
            height: 29
            radius: 6
            color: root.c("primary")
            Text {
                anchors.centerIn: parent
                text: "Open"
                font.pointSize: 9
                color: root.c("onPrimary")
            }
        }
        Rectangle {
            x: parent.width - 105
            y: root.condensed ? 76 : 84
            width: 90
            height: 3
            color: root.c("secondary")
        }
    }
    Rectangle {
        x: 14
        visible: !root.compact
        y: root.height - height - 10
        width: (parent.width - 34) * .52
        height: root.condensed ? 68 : 82
        radius: 7
        objectName: "previewTerminal"
        color: root.c("surfaceContainerLowest")
        Text {
            x: 9
            y: root.condensed ? 7 : 9
            text: "$ build"
            font.family: "JetBrains Mono NF"
            font.pointSize: 8
            color: root.c("onSurface")
        }
        Text {
            x: 9
            y: root.condensed ? 27 : 31
            text: "✓ Ready"
            font.pointSize: 8
            color: root.c("green")
        }
        Text {
            x: 9
            y: root.condensed ? 45 : 53
            text: "! Warning"
            font.pointSize: 8
            color: root.c("yellow")
        }
    }
    Rectangle {
        x: parent.width * .57
        visible: !root.compact
        y: root.height - height - 10
        width: parent.width * .43 - 14
        height: 67
        radius: 8
        objectName: "previewNotification"
        color: root.c("primaryContainer")
        Text {
            x: 9
            y: 9
            width: parent.width - 18
            text: "Message\nYour theme is ready"
            wrapMode: Text.Wrap
            font.pointSize: 8
            color: root.c("onPrimaryContainer")
        }
    }
    Accessible.role: Accessible.Graphic
    Accessible.name: "Desktop preview with bar, window, terminal and notification"
}
