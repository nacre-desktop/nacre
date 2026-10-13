import QtQuick
import QtQuick.Controls.Basic
import qs.widgets
import qs.services

Flickable {
    id: root
    required property bool shouldUpdate
    required property var visibilities
    readonly property real canvasWidth: Math.min(width, 1700)
    readonly property bool narrow: canvasWidth < 650
    readonly property real summaryHeight: Math.max(288, Math.min(420, height * .38))
    readonly property real heroHeight: Math.max(220, Math.min(640, canvasWidth * .34, height - summaryHeight - 18))
    readonly property string heroVideoPath: /\.(mp4|webm|mkv|mov|m4v)$/i.test(NacreWallpapers.displayPreview) ? NacreWallpapers.displayPreview : NacreWallpapers.displayPath
    implicitWidth: 940
    implicitHeight: 550
    contentHeight: sections.height
    contentWidth: width
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: NacreScrollBar {}
    FastScroll {
        view: root
    }
    Column {
        id: sections
        width: root.canvasWidth
        x: (root.width - width) / 2
        spacing: 18
        NacreDashboardHero {
            id: hero
            objectName: "dashboardWallpaperHero"
            width: parent.width
            height: root.heroHeight
            Image {
                anchors.fill: parent
                source: NacreWallpapers.poster ? "file://" + NacreWallpapers.poster : ""
                fillMode: Image.PreserveAspectCrop
                asynchronous: true
                cache: true
                retainWhileLoading: true
                sourceSize.width: 1440
                sourceSize.height: 960
            }
            Loader {
                anchors.fill: parent
                active: root.shouldUpdate && NacreWallpapers.displayDynamic && !NacreWallpapers.displayAnimated && NacreTokens.motionEnabled && !WallpaperPlayback.batteryPaused && !WallpaperPlayback.paused && !WallpaperPlayback.sleeping && !WallpaperPlayback.locked
                source: active ? Qt.resolvedUrl("NacreDashboardVideo.qml") : ""
                onLoaded: item.path = Qt.binding(() => root.heroVideoPath)
            }

            AnimatedImage {
                anchors.fill: parent
                source: root.shouldUpdate && NacreWallpapers.displayDynamic && NacreWallpapers.displayAnimated ? "file://" + NacreWallpapers.displayPath : ""
                playing: root.shouldUpdate && NacreTokens.motionEnabled && !WallpaperPlayback.batteryPaused && !WallpaperPlayback.paused
                visible: status === Image.Ready
                fillMode: Image.PreserveAspectCrop
            }
            Rectangle {
                anchors.fill: parent
                color: "#11151b"
                opacity: .58
            }
            Rectangle {
                anchors.fill: parent
                gradient: Gradient {
                    GradientStop {
                        position: 0
                        color: "#0011151b"
                    }
                    GradientStop {
                        position: 1
                        color: "#a011151b"
                    }
                }
            }
            Column {
                x: 24
                y: 24
                width: parent.width - 48
                spacing: 10
                NacreText {
                    text: NacreTime.format("HH:mm")
                    font.pointSize: Math.max(44, Math.min(64, 44 * hero.height / 254))
                    font.weight: Font.Light
                    color: "#fffdfa"
                }
                NacreText {
                    text: NacreTime.format(NacreI18n.tr("dashboard.today-page.text.dddd-d-mmmm", "dddd, d MMMM"))
                    font.pointSize: 13
                    color: "#fffdfa"
                }
                Row {
                    spacing: 8
                    NacreIcon {
                        text: NacreWeather.icon
                        color: "#fffdfa"
                    }
                    NacreText {
                        text: NacreWeather.displayTemperature ? NacreWeather.displayTemperature + "  " + NacreWeather.description + " · " + NacreWeather.location : NacreI18n.tr("dashboard.today-page.text.weather-unavailable", "Weather unavailable")
                        color: "#fffdfa"
                        font.pointSize: 11
                    }
                }
            }
            NacreText {
                anchors.right: parent.right
                anchors.rightMargin: 24
                y: 38
                text: NacreDashboardState.home.time ? NacreI18n.tr("ui.today-page.text.home-value0-value1", "Home · {value0}  {value1}", {
                    value0: NacreDashboardState.home.label,
                    value1: NacreDashboardState.home.time
                }) : ""
                color: "#fffdfa"
                font.pointSize: 11
                visible: !root.narrow
            }
            Row {
                objectName: "dashboardPaletteStrip"
                visible: root.width >= 360
                x: 24
                anchors.bottom: parent.bottom
                anchors.bottomMargin: 18
                spacing: 10
                Repeater {
                    model: [NacreColours.palette.m3primary, NacreColours.palette.m3secondary, NacreColours.palette.m3tertiary, NacreColours.palette.m3surfaceContainerHigh, NacreColours.palette.m3onSurface].slice(0, root.width < 440 ? 3 : 5)
                    Rectangle {
                        required property var modelData
                        width: 19
                        height: 19
                        radius: 9.5
                        color: modelData
                        anchors.verticalCenter: parent.verticalCenter
                        NacreInteraction {
                            accessibleName: NacreI18n.tr("dashboard.today-page.accessibleName.open-wallpaper-colors", "Open wallpaper colors")
                            function onClicked() {
                                NacreColorsApp.open();
                            }
                        }
                    }
                }
                NacreText {
                    text: NacreI18n.option("personality-value", NacreWallpapers.preferences.palettePersonality || "natural") + " · " + (NacreColours.light ? NacreI18n.tr("dashboard.today-page.text.light", "Light") : NacreI18n.tr("dashboard.today-page.text.dark", "Dark"))
                    color: "#fffdfa"
                    font.pointSize: 10
                    anchors.verticalCenter: parent.verticalCenter
                    visible: !root.narrow
                }
            }
            Column {
                id: wallpaperActions
                anchors.right: parent.right
                anchors.rightMargin: 18
                anchors.bottom: parent.bottom
                anchors.bottomMargin: 12
                width: Math.max(0, Math.min(228, parent.width - 36))
                spacing: 6
                Row {
                    id: options
                    width: parent.width
                    spacing: 6
                    WallpaperAction {
                        objectName: "dashboardWallpapers"
                        text: NacreI18n.tr("dashboard.today-page.text.wallpapers", "Wallpapers")
                        icon: "wallpaper"
                        compact: true
                        onClicked: NacrePanelState.openMode("wallpaper")
                    }
                    WallpaperAction {
                        objectName: "dashboardColors"
                        text: NacreI18n.tr("dashboard.today-page.text.colors", "Colors")
                        icon: "palette"
                        compact: true
                        onClicked: NacreColorsApp.open()
                    }
                }
                NacreSurface {
                    objectName: "dashboardWallpaperTransport"
                    anchors.horizontalCenter: parent.horizontalCenter
                    width: parent.width
                    height: 38
                    radius: 12
                    color: Qt.tint(NacreTokens.body, Qt.alpha(NacreTokens.orientSecondary, .1))
                    Item {
                        anchors.fill: parent
                        anchors.margins: 3
                        WallpaperControl {
                            objectName: "dashboardPreviousWallpaper"
                            icon: "chevron_left"
                            label: NacreI18n.tr("dashboard.today-page.label.previous-wallpaper", "Previous wallpaper")
                            available: NacreWallpapers.rotationReady
                            onActivated: NacreWallpapers.previousWallpaper()
                        }
                        WallpaperControl {
                            x: parent.width / 3
                            objectName: "dashboardPauseWallpaper"
                            icon: !NacreWallpapers.displayDynamic ? "image" : WallpaperPlayback.paused ? "play_arrow" : "pause"
                            label: !NacreWallpapers.displayDynamic ? NacreI18n.tr("dashboard.today-page.label.static-wallpaper", "Static wallpaper") : WallpaperPlayback.paused ? NacreI18n.tr("dashboard.today-page.label.resume-wallpaper", "Resume wallpaper") : NacreI18n.tr("dashboard.today-page.label.pause-wallpaper", "Pause wallpaper")
                            emphasized: true
                            informational: !NacreWallpapers.displayDynamic
                            available: NacreWallpapers.displayDynamic && !NacreWallpapers.themeBusy && !WallpaperPlayback.sleeping && !WallpaperPlayback.locked
                            onActivated: NacreWallpapers.toggleMotion()
                        }
                        WallpaperControl {
                            x: parent.width * 2 / 3
                            objectName: "dashboardNextWallpaper"
                            icon: "chevron_right"
                            label: NacreI18n.tr("dashboard.today-page.label.next-wallpaper", "Next wallpaper")
                            available: NacreWallpapers.rotationReady
                            onActivated: NacreWallpapers.advanceRotation(true)
                        }
                    }
                }
            }
        }
        NacreDashboardSummary {
            objectName: "dashboardSummary"
            width: parent.width
            height: root.narrow ? implicitHeight : root.summaryHeight
            active: root.shouldUpdate
            onSystemRequested: root.visibilities.dashboardTab = 3
        }
    }
    component WallpaperAction: ActionButton {
        width: (parent.width - parent.spacing) / 2
        height: 36
        radius: 10
        color: Qt.tint(NacreTokens.body, Qt.alpha(NacreTokens.accent, .12))
    }
    component WallpaperControl: Item {
        id: control
        required property string icon
        required property string label
        property bool available: false
        property bool informational: false
        property bool emphasized: false
        signal activated
        width: parent.width / 3
        height: parent.height
        Accessible.role: Accessible.Indicator
        Accessible.name: informational ? label : ""
        Accessible.ignored: !informational
        NacreSurface {
            id: centerBadge
            objectName: "wallpaperCenterBadge"
            anchors.centerIn: parent
            width: parent.width
            height: parent.height
            radius: 9
            color: "transparent"
            visible: control.emphasized
            HoverHandler {
                id: infoHover
                enabled: control.informational
            }
        }
        NacreIcon {
            anchors.centerIn: parent
            text: parent.icon
            font.pointSize: 16
            color: NacreTokens.ink
            opacity: control.informational || control.available ? 1 : .4
        }
        NacreInteraction {
            id: interaction
            anchors.fill: control.emphasized ? centerBadge : parent
            radius: 9
            visible: !control.informational
            disabled: !parent.available
            accessibleName: parent.label
            function onClicked(event) {
                if (event)
                    focus = false;
                parent.activated();
            }
        }
        ToolTip {
            id: tooltip
            visible: control.informational ? infoHover.hovered : interaction.hovered && interaction.enabled
            delay: 500
            text: parent.label
            contentItem: NacreText {
                text: tooltip.text
                font.pointSize: 10
            }
            background: NacreSurface {
                color: NacreTokens.raised
                radius: 8
            }
        }
    }
}
