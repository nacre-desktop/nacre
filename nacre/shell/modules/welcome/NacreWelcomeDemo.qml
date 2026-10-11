import QtQuick
import qs.widgets
import qs.services
import "../colors"

Column {
    id: root
    spacing: 10
    property real availableHeight: 600
    readonly property var entries: NacreWelcomeApp.demoEntries
    Row {
        id: scenes
        objectName: "welcomeScenes"
        width: parent.width
        spacing: 12
        Repeater {
            model: root.entries
            Column {
                id: tile
                required property var modelData
                required property int index
                width: (scenes.width - 36) / 4
                spacing: 7
                NacreSurface {
                    id: card
                    objectName: "welcomeScene_" + tile.index
                    width: parent.width
                    height: Math.max(70, Math.min(180, width * .56, root.availableHeight * .22))
                    radius: 12
                    color: NacreTokens.raised
                    border.width: selected ? 2 : 1
                    border.color: selected ? Qt.alpha(NacreTokens.ink, .65) : Qt.alpha(NacreTokens.outline, .4)
                    readonly property bool selected: (NacreWallpapers.selectedPath || NacreWallpapers.displayPath) === tile.modelData.path
                    enabled: !NacreWelcomeApp.demoBusy
                    NacreClip {
                        x: 4
                        y: 4
                        width: parent.width - 8
                        height: parent.height - 8
                        radius: 9
                        Image {
                            anchors.fill: parent
                            source: "file://" + (tile.modelData.preview || tile.modelData.poster)
                            fillMode: Image.PreserveAspectCrop
                            asynchronous: true
                            cache: true
                            retainWhileLoading: true
                            sourceSize: Qt.size(512, 288)
                        }
                        NacreSurface {
                            anchors.right: parent.right
                            anchors.bottom: parent.bottom
                            anchors.margins: 6
                            width: 24
                            height: 24
                            radius: 12
                            visible: card.selected
                            color: NacreTokens.body
                            NacreIcon {
                                anchors.centerIn: parent
                                text: "check"
                                font.pointSize: 10
                            }
                        }
                    }
                    NacreInteraction {
                        accessibleName: "Use wallpaper " + tile.modelData.name
                        function onClicked() {
                            NacreWelcomeApp.selectDemo(tile.modelData.path);
                        }
                    }
                }
                NacreText {
                    width: parent.width
                    text: tile.modelData.name
                    height: Math.max(34, implicitHeight)
                    font.pointSize: 10
                    maximumLineCount: 2
                    wrapMode: Text.Wrap
                    elide: Text.ElideRight
                }
                NacreText {
                    width: parent.width
                    visible: !!tile.modelData.artist
                    textFormat: Text.RichText
                    font.pointSize: 8.5
                    wrapMode: Text.Wrap
                    color: NacreTokens.mutedInk
                    linkColor: NacreTokens.mutedInk
                    text: tile.modelData.artist ? "<a href=\"" + tile.modelData.source + "\">" + tile.modelData.artist + "</a> · " + tile.modelData.license.replace(/-/g, " ") : ""
                    onLinkActivated: url => NacreWelcomeApp.openCredit(url)
                }
            }
        }
    }
    NacreText {
        visible: !root.entries.length
        text: NacreWelcomeApp.demoStarting ? "Preparing the demo…" : NacreWelcomeApp.demoError
        font.pointSize: 10
        color: NacreTokens.mutedInk
    }
    Item {
        id: styleControls
        width: parent.width
        height: Math.max(styleFlow.implicitHeight, 38)
        Flow {
            id: styleFlow
            width: parent.width - 90
            spacing: 6
            NacreText {
                text: "Style"
                font.pointSize: 11
                height: 34
                verticalAlignment: Text.AlignVCenter
                rightPadding: 8
            }
            Repeater {
                model: ["natural", "pigment", "harmony", "pop", "mist", "vivid", "pearl", "tide"]
                ActionButton {
                    required property string modelData
                    objectName: "welcomePersonality_" + modelData
                    text: modelData.charAt(0).toUpperCase() + modelData.slice(1)
                    compact: true
                    selected: (NacreWelcomeApp.requestedPersonality || NacreWelcomeApp.appearance.palettePersonality) === modelData
                    enabled: !NacreWelcomeApp.demoBusy
                    onClicked: NacreWelcomeApp.themeDemo("", modelData)
                }
            }
        }
        Row {
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            spacing: 4
            Repeater {
                model: ["dark", "light"]
                ActionButton {
                    required property string modelData
                    objectName: "welcomeMode_" + modelData
                    text: ""
                    accessibleLabel: modelData === "dark" ? "Dark colors" : "Light colors"
                    icon: modelData === "dark" ? "dark_mode" : "light_mode"
                    compact: true
                    selected: (NacreWelcomeApp.requestedMode || NacreWelcomeApp.appearance.paletteMode) === modelData
                    enabled: !NacreWelcomeApp.demoBusy
                    onClicked: NacreWelcomeApp.themeDemo(modelData, "")
                }
            }
        }
    }
    NacreText {
        id: styleDescription
        width: parent.width
        wrapMode: Text.Wrap
        text: ({
                natural: "The original balance of the wallpaper's own colors",
                pigment: "Colors balanced by image coverage and detail",
                harmony: "Related accents for a calm, unified desktop",
                mist: "Soft, nearly neutral colors for focus",
                vivid: "Stronger colors, with readable text",
                tide: "Colors gently warmed or cooled with the time of day",
                pop: "A bold contrasting detail becomes the accent",
                pearl: "Neutral surfaces and Nacre's mother-of-pearl tones"
            })[NacreWelcomeApp.requestedPersonality || NacreWelcomeApp.appearance.palettePersonality] || "Your chosen color style"
        font.pointSize: 10
        color: NacreTokens.mutedInk
    }
    NacreThemePreview {
        active: NacreWelcomeApp.visible && root.visible
        objectName: "welcomeDesktopPreview"
        width: parent.width
        height: Math.max(210, Math.min(420, root.availableHeight - scenes.implicitHeight - styleControls.height - styleDescription.implicitHeight - paletteRoles.height - root.spacing * 4))
        palette: NacrePresentation.active.colours || {}
        wallpaper: NacreWallpapers.displayPreview || ""
    }
    Flow {
        id: paletteRoles
        objectName: "welcomePaletteRoles"
        width: parent.width
        spacing: 12
        Repeater {
            model: [
                {
                    role: "primary",
                    name: "Main"
                },
                {
                    role: "secondary",
                    name: "Second"
                },
                {
                    role: "tertiary",
                    name: "Third"
                },
                {
                    role: "surface",
                    name: "Background"
                }
            ]
            Row {
                required property var modelData
                width: Math.max(140, (paletteRoles.width - 36) / 4)
                height: 34
                spacing: 10
                Rectangle {
                    width: 34
                    height: 34
                    radius: 9
                    color: "#" + (NacrePresentation.active.colours?.[parent.modelData.role] || "202020")
                    border.width: 1
                    border.color: NacreTokens.outline
                }
                NacreText {
                    width: parent.width - 44
                    height: parent.height
                    verticalAlignment: Text.AlignVCenter
                    text: parent.modelData.name
                    font.pointSize: 10
                    elide: Text.ElideRight
                }
            }
        }
    }
    NacreText {
        width: parent.width
        text: NacreWelcomeApp.demoError || (NacreWelcomeApp.demoBusy ? "Applying to your desktop…" : "")
        visible: !!text
        font.pointSize: 10
        wrapMode: Text.Wrap
        color: NacreTokens.mutedInk
    }
}
