import "widgets"
import "services"
import Quickshell
import Quickshell.Io
import QtQuick

ShellRoot {
    FloatingWindow {
        id: window
        objectName: "lockPreviewWindow"
        visible: true
        title: "Nacre lock screen preview"
        implicitWidth: 1280
        implicitHeight: 800
        color: NacreColours.palette.m3surface
        property var widgetData: ({})
        Image {
            anchors.fill: parent
            source: window.widgetData.wallpaper ? "file://" + window.widgetData.wallpaper : (NacreWallpapers.poster ? "file://" + NacreWallpapers.poster : "")
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
        }
        Rectangle {
            anchors.fill: parent
            color: "#0d000000"
        }
        Item {
            objectName: "lockPreviewContent"
            anchors.fill: parent
            focus: true
            Keys.onEscapePressed: Qt.quit()
            NacreText {
                x: 24
                y: 20
                text: "Lock screen preview · Esc to close"
                color: "white"
                font.pointSize: 11
            }
            Item {
                id: panel
                width: 1440
                height: 810
                anchors.centerIn: parent
                scale: Math.min(window.width / 2048, window.height / 1152)
                property color accent: window.widgetData.colors?.primary ? "#" + window.widgetData.colors.primary : NacreColours.palette.m3primary
                property color foreground: window.widgetData.colors?.onSurface ? "#" + window.widgetData.colors.onSurface : NacreColours.palette.m3onSurface
                Image {
                    anchors.fill: parent
                    source: window.widgetData.panel ? "file://" + window.widgetData.panel : ""
                    cache: false
                }
                Text {
                    x: 608
                    y: 53
                    width: 126
                    height: 134
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                    objectName: "lockPreviewHour"
                    text: NacreTime.format("hh AP").split(" ")[0]
                    font.family: "IBM Plex Sans"
                    font.pixelSize: 104
                    color: panel.accent
                }
                Text {
                    x: 721
                    y: 68
                    width: 90
                    height: 72
                    horizontalAlignment: Text.AlignHCenter
                    text: NacreTime.format("mm")
                    font.family: "IBM Plex Sans"
                    font.pixelSize: 54
                    color: panel.accent
                }
                Text {
                    x: 721
                    y: 133
                    width: 90
                    horizontalAlignment: Text.AlignHCenter
                    text: NacreTime.format("AP")
                    font.family: "IBM Plex Sans"
                    font.pixelSize: 27
                    color: panel.foreground
                }
                Text {
                    x: 495
                    y: 180
                    width: 450
                    horizontalAlignment: Text.AlignHCenter
                    text: NacreTime.format("dddd · dd MMM").toUpperCase()
                    font.family: "IBM Plex Sans"
                    font.pixelSize: 20
                    color: panel.foreground
                }
                Rectangle {
                    x: 590
                    y: 602
                    width: 260
                    height: 46
                    radius: 23
                    color: NacreColours.palette.m3surfaceContainer
                    opacity: 0.8
                    border.width: 1
                    border.color: panel.accent
                    Text {
                        anchors.centerIn: parent
                        text: "Enter your password  →"
                        font.family: "IBM Plex Sans"
                        font.pixelSize: 14
                        color: panel.foreground
                    }
                }
                Row {
                    x: 649
                    y: 676
                    spacing: 72
                    NacreIcon {
                        text: "bedtime"
                        font.pointSize: 18
                        color: panel.accent
                    }
                    NacreIcon {
                        text: "lock"
                        font.pointSize: 18
                        color: panel.accent
                    }
                }
                Repeater {
                    model: window.widgetData.preferences?.lockMedia === false ? [] : [
                        {
                            x: 165,
                            icon: "skip_previous",
                            action: "previous"
                        },
                        {
                            x: 238,
                            icon: window.widgetData.media?.playing ? "pause" : "play_arrow",
                            action: "toggle"
                        },
                        {
                            x: 311,
                            icon: "skip_next",
                            action: "next"
                        }
                    ]
                    Item {
                        required property var modelData
                        x: modelData.x - 20
                        y: 667
                        width: 40
                        height: 40
                        NacreIcon {
                            anchors.centerIn: parent
                            text: parent.modelData.icon
                            font.pointSize: parent.modelData.action === "toggle" ? 21 : 16.5
                            color: parent.modelData.action === "toggle" ? NacreColours.palette.m3onPrimary : panel.foreground
                        }
                        MouseArea {
                            anchors.fill: parent
                            onClicked: AppLaunch.run(["python3", Quickshell.env("HOME") + "/.local/share/nacre/shell/tools/lock-info.py", parent.modelData.action])
                        }
                    }
                }
            }
        }
        Process {
            id: reader
            running: true
            command: ["/usr/bin/cat", Quickshell.env("HOME") + "/.cache/nacre/lock-ready/dashboard.json"]
            stdout: SplitParser {
                splitMarker: ""
                onRead: line => {
                    try {
                        window.widgetData = JSON.parse(line);
                    } catch (e) {}
                }
            }
        }
        Timer {
            interval: 3000
            repeat: true
            running: window.visible
            onTriggered: if (!reader.running)
                reader.running = true
        }
    }
}
