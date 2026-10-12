import QtQuick
import QtQuick.Controls
import qs.widgets
import qs.services
import "welcome-catalog.js" as Catalog

Item {
    id: root
    focus: true
    readonly property bool narrow: width < 820
    function captureState() {
        return {
            page: NacreWelcomeApp.page,
            allShortcuts: allShortcuts,
            scroll: canvas.contentY
        };
    }
    function restoreState(data) {
        allShortcuts = data.allShortcuts === true;
        Qt.callLater(() => canvas.contentY = data.page === NacreWelcomeApp.page ? data.scroll || 0 : 0);
    }
    property bool allShortcuts: false
    property real pageOffset: 0
    property real pageOpacity: 1
    property int previousStep: 0
    Keys.onEscapePressed: NacreWelcomeApp.close()
    Keys.onReturnPressed: if (!NacreWelcomeApp.demoBusy)
        NacreWelcomeApp.advance()
    Keys.onEnterPressed: if (!NacreWelcomeApp.demoBusy)
        NacreWelcomeApp.advance()
    function page(name) {
        NacreWelcomeApp.jump(name);
        canvas.contentY = 0;
    }
    Connections {
        target: NacreWelcomeApp
        function onPageChanged() {
            wheel.cancel();
            wheel.destination = 0;
            canvas.contentY = 0;
            if (NacreTokens.motionEnabled) {
                root.pageOffset = NacreWelcomeApp.step >= root.previousStep ? 24 : -24;
                root.pageOpacity = 0;
                Qt.callLater(() => {
                    root.pageOffset = 0;
                    root.pageOpacity = 1;
                });
            }
            root.previousStep = NacreWelcomeApp.step;
        }
    }
    Flickable {
        id: canvas
        objectName: "welcomeCanvas"
        width: parent.width
        height: Math.max(0, footer.y - 16)
        contentWidth: width
        contentHeight: body.implicitHeight
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        maximumFlickVelocity: 6000
        FastScroll {
            id: wheel
            objectName: "welcomeWheel"
            view: canvas
            step: 900
            pixelMultiplier: 3
            kinetic: false
            smoothDuration: 80
        }
        ScrollBar.vertical: NacreScrollBar {
            id: scrollBar
            onPressedChanged: if (pressed)
                wheel.cancel()
        }
        Column {
            id: body
            width: canvas.width
            spacing: 14
            x: root.pageOffset
            opacity: root.pageOpacity
            Behavior on x {
                enabled: NacreTokens.motionEnabled
                NumberAnimation {
                    duration: 170
                    easing.type: Easing.OutCubic
                }
            }
            Behavior on opacity {
                enabled: NacreTokens.motionEnabled
                NumberAnimation {
                    duration: 150
                }
            }
            Column {
                visible: NacreWelcomeApp.page === "welcome"
                width: parent.width
                spacing: 24
                Item {
                    width: 1
                    height: Math.max(18, canvas.height * .12)
                }
                BrandLogo {
                    anchors.horizontalCenter: parent.horizontalCenter
                    objectName: "welcomeHeroLogo"
                    width: Math.max(180, Math.min(320, canvas.height * .38))
                    height: width
                    compact: false
                    motionEnabled: NacreTokens.motionEnabled
                }
                NacreText {
                    width: parent.width
                    text: NacreI18n.tr("welcome.welcome-view.text.nacre", "Nacre")
                    font.pointSize: Math.max(30, Math.min(46, canvas.height * .065))
                    font.weight: Font.Medium
                    horizontalAlignment: Text.AlignHCenter
                }
                NacreText {
                    width: parent.width
                    text: NacreI18n.tr("welcome.welcome-view.text.mother-of-pearl-colors-shaped-by-your-wallpaper", "Mother-of-pearl. Colors shaped by your wallpaper.")
                    font.pointSize: Math.max(13, Math.min(17, root.width / 100))
                    wrapMode: Text.Wrap
                    horizontalAlignment: Text.AlignHCenter
                }
                NacreText {
                    width: parent.width
                    text: NacreI18n.tr("welcome.welcome-view.text.a-short-tour-of-your-desktop", "A short tour of your desktop")
                    font.pointSize: 11
                    color: NacreTokens.mutedInk
                    horizontalAlignment: Text.AlignHCenter
                }
            }
            Column {
                visible: NacreWelcomeApp.page === "colors"
                width: parent.width
                spacing: 12
                NacreText {
                    text: NacreI18n.tr("welcome.welcome-view.text.your-colors", "Your colors")
                    font.pointSize: 25
                }
                NacreText {
                    width: parent.width
                    text: NacreI18n.tr("welcome.welcome-view.text.pick-a-wallpaper-and-watch-your-whole-desktop-change-li", "Pick a wallpaper and watch your whole desktop change live.")
                    wrapMode: Text.Wrap
                    color: NacreTokens.mutedInk
                }
                NacreWelcomeDemo {
                    width: parent.width
                    availableHeight: Math.max(300, canvas.height - 90)
                }
            }
            Column {
                visible: NacreWelcomeApp.page === "shortcuts"
                width: parent.width
                spacing: 14
                NacreText {
                    width: parent.width
                    wrapMode: Text.Wrap
                    text: NacreI18n.tr("welcome.welcome-view.text.getting-around", "Getting around")
                    font.pointSize: 25
                }
                NacreText {
                    width: parent.width
                    wrapMode: Text.Wrap
                    text: NacreI18n.tr("welcome.welcome-view.text.your-current-shortcuts-grouped-by-what-they-do-scroll-t", "Your current shortcuts, grouped by what they do. Scroll to explore.")
                    font.pointSize: 11
                    color: NacreTokens.mutedInk
                }
                Repeater {
                    model: ["Apps and panels", "Windows", "Workspaces and focus", "Capture and clipboard", "Sound, display and locking"]
                    Column {
                        required property string modelData
                        readonly property var rows: (NacreWelcomeApp.data.shortcuts || []).filter(item => (item.group || "Apps and panels") === modelData)
                        width: body.width
                        spacing: 8
                        visible: rows.length > 0
                        NacreText {
                            text: NacreI18n.option("shortcut-group", parent.modelData)
                            font.pointSize: 13
                            topPadding: 8
                        }
                        Repeater {
                            model: parent.rows
                            NacreSurface {
                                required property var modelData
                                objectName: "welcomeShortcut_" + modelData.id
                                width: body.width
                                height: Math.max(58, Math.max(keyLabel.implicitHeight, shortcutTitle.implicitHeight) + 24)
                                radius: 10
                                color: NacreTokens.raised
                                NacreText {
                                    id: shortcutTitle
                                    x: 14
                                    y: 12
                                    width: parent.width * .43 - 20
                                    text: NacreI18n.modelLabel(modelData, "title")
                                    font.pointSize: 10.5
                                    wrapMode: Text.Wrap
                                }
                                NacreText {
                                    id: keyLabel
                                    x: parent.width * .43
                                    y: 12
                                    width: parent.width - x - 14
                                    text: modelData.key
                                    font.family: NacreTokens.monoFamily
                                    font.pointSize: 10.5
                                    color: NacreTokens.accent
                                    wrapMode: Text.Wrap
                                }
                            }
                        }
                    }
                }
                NacreText {
                    width: parent.width
                    wrapMode: Text.Wrap
                    text: NacreWelcomeApp.data.shortcutError || NacreI18n.tr("welcome.welcome-view.text.super-is-usually-the-windows-key", "Super is usually the Windows key.")
                    font.pointSize: 10
                    color: NacreTokens.mutedInk
                }
            }
            Column {
                width: parent.width
                spacing: 14
                visible: NacreWelcomeApp.page === "apps"
                NacreText {
                    width: parent.width
                    wrapMode: Text.Wrap
                    text: NacreI18n.tr("welcome.welcome-view.text.the-nacre-suite", "The Nacre suite")
                    font.pointSize: 23
                }
                NacreText {
                    width: parent.width
                    text: NacreI18n.tr("welcome.welcome-view.text.tools-for-longer-tasks-with-quick-controls-always-close", "Tools for longer tasks, with quick controls always close by in the shell.")
                    color: NacreTokens.mutedInk
                    wrapMode: Text.Wrap
                }
                NacreSurface {
                    id: setupGuide
                    width: parent.width
                    visible: !!NacreWelcomeApp.optionalApp
                    implicitHeight: setupText.implicitHeight + 32
                    radius: 12
                    color: NacreTokens.raised
                    readonly property var guide: NacreI18n.localize(Catalog.optional[NacreWelcomeApp.optionalApp] || {})
                    Column {
                        id: setupText
                        x: 16
                        y: 16
                        width: parent.width - 32
                        spacing: 10
                        NacreText {
                            width: parent.width
                            text: setupGuide.guide.title || ""
                            font.pointSize: 16
                            wrapMode: Text.Wrap
                        }
                        NacreText {
                            width: parent.width
                            text: setupGuide.guide.detail || ""
                            wrapMode: Text.Wrap
                        }
                        NacreText {
                            width: parent.width
                            text: setupGuide.guide.setup || ""
                            wrapMode: Text.Wrap
                            color: NacreTokens.mutedInk
                        }
                        NacreText {
                            width: parent.width
                            text: setupGuide.guide.status || ""
                            wrapMode: Text.Wrap
                            color: NacreTokens.mutedInk
                            font.pointSize: 10.5
                        }
                        ActionButton {
                            text: NacreI18n.tr("welcome.welcome-view.text.back-to-apps", "Back to apps")
                            onClicked: NacreWelcomeApp.optionalApp = ""
                        }
                    }
                }
                Repeater {
                    model: NacreI18n.localize(Catalog.apps)
                    Item {
                        required property var modelData
                        width: body.width
                        height: appRow.implicitHeight
                        BrandLogo {
                            id: appLogo
                            x: 14
                            y: 20
                            width: 44
                            height: 44
                            ai: modelData.logo === "ai"
                            brain: modelData.logo === "brain"
                            settings: modelData.logo === "settings"
                            colorsApp: modelData.logo === "colors"
                            motionEnabled: NacreTokens.motionEnabled
                            opacity: appLogo.ready ? (appRow.optional ? .4 : 1) : 0
                            z: 1
                        }
                        NacreWelcomeRow {
                            id: appRow
                            objectName: "welcomeAppRow_" + modelData.logo
                            width: parent.width
                            heading: modelData.title
                            leadingSpace: 76
                            icon: modelData.icon
                            showGlyph: !appLogo.ready
                            optional: !!modelData.availability && NacreWelcomeApp.data.available?.[modelData.availability] !== true
                            buttonText: optional ? NacreI18n.tr("welcome.welcome-view.buttonText.setup-guide", "Setup guide") : NacreI18n.tr("welcome.welcome-view.buttonText.open", "Open")
                            available: true
                            detail: optional ? NacreI18n.tr("welcome.application.optional", "Optional. {detail}", {
                                detail: modelData.detail
                            }) : modelData.detail
                            onClicked: {
                                if (optional) {
                                    NacreWelcomeApp.setup(modelData.availability);
                                    Qt.callLater(() => canvas.contentY = 0);
                                } else
                                    NacreWelcomeApp.route(modelData.action);
                            }
                        }
                    }
                }
                NacreText {
                    width: parent.width
                    text: NacreI18n.tr("welcome.welcome-view.text.ai-and-brain-are-optional-welcome-uses-what-is-already", "AI and Brain are optional. Welcome uses what is already installed; it does not set up accounts or download models.")
                    color: NacreTokens.mutedInk
                    wrapMode: Text.Wrap
                    font.pointSize: 10.5
                }
            }

            Item {
                visible: NacreWelcomeApp.page === "ready"
                width: parent.width
                height: Math.max(0, (canvas.height - 340) / 2)
            }
            Column {
                visible: NacreWelcomeApp.page === "ready"
                width: parent.width
                spacing: 18
                BrandLogo {
                    anchors.horizontalCenter: parent.horizontalCenter
                    width: 120
                    height: 120
                    compact: false
                }
                NacreText {
                    width: parent.width
                    text: NacreI18n.tr("welcome.welcome-view.text.you-re-all-set", "You're all set.")
                    font.pointSize: 28
                    horizontalAlignment: Text.AlignHCenter
                }
                NacreText {
                    width: parent.width
                    text: (NacreWelcomeApp.demoEntries.find(item => item.path === NacreWallpapers.displayPath)?.name || NacreWallpapers.currentEntry?.name || NacreI18n.tr("welcome.welcome-view.text.your-wallpaper", "Your wallpaper")) + " · " + NacreI18n.option("personality", NacreWelcomeApp.appearance.palettePersonality || "natural") + " · " + NacreI18n.option("mode", NacreWelcomeApp.appearance.paletteMode || "dark")
                    font.pointSize: 12
                    color: NacreTokens.mutedInk
                    wrapMode: Text.Wrap
                    horizontalAlignment: Text.AlignHCenter
                }
                NacreText {
                    width: parent.width
                    text: NacreI18n.tr("welcome.welcome-view.text.your-choices-are-saved-reopen-welcome-from-the-launcher", "Your choices are saved. Reopen Welcome from the launcher whenever you like.")
                    color: NacreTokens.mutedInk
                    font.pointSize: 10.5
                    wrapMode: Text.Wrap
                    horizontalAlignment: Text.AlignHCenter
                }
                Row {
                    anchors.horizontalCenter: parent.horizontalCenter
                    spacing: 10
                    ActionButton {
                        text: NacreI18n.tr("welcome.welcome-view.text.help", "Help")
                        compact: true
                        onClicked: root.page("help")
                    }
                }
            }
            Column {
                width: parent.width
                spacing: 14
                visible: NacreWelcomeApp.page === "help"
                NacreText {
                    text: NacreI18n.tr("welcome.welcome-view.text.a-little-guidance-when-you-need-it", "A little guidance, when you need it")
                    font.pointSize: 23
                }
                Repeater {
                    model: NacreI18n.localize(Catalog.help)
                    Column {
                        required property var modelData
                        width: body.width
                        spacing: 6
                        NacreText {
                            width: parent.width
                            text: modelData.title
                            font.pointSize: 13
                            wrapMode: Text.Wrap
                        }
                        NacreText {
                            width: parent.width
                            text: modelData.detail
                            color: NacreTokens.mutedInk
                            font.pointSize: 11
                            wrapMode: Text.Wrap
                        }
                        Rectangle {
                            width: parent.width
                            height: 1
                            color: NacreTokens.outline
                            opacity: .2
                        }
                    }
                }
                Flow {
                    width: parent.width
                    spacing: 8
                    ActionButton {
                        text: NacreI18n.tr("welcome.welcome-view.text.maintenance", "Maintenance")
                        onClicked: NacreWelcomeApp.route("settings:maintenance")
                    }
                    ActionButton {
                        text: NacreI18n.tr("welcome.welcome-view.text.display-setup", "Display setup")
                        onClicked: NacreWelcomeApp.route("settings:displays")
                    }
                }
                NacreText {
                    text: NacreI18n.tr("welcome.welcome-view.text.read-more-online", "Read more online")
                    font.pointSize: 15
                }
                Flow {
                    width: parent.width
                    spacing: 8
                    ActionButton {
                        text: NacreI18n.tr("welcome.welcome-view.text.nacre-project", "Nacre project")
                        onClicked: NacreWelcomeApp.link("project")
                    }
                    ActionButton {
                        text: NacreI18n.tr("welcome.welcome-view.text.hyprland-guide", "Hyprland guide")
                        onClicked: NacreWelcomeApp.link("hyprland")
                    }
                    ActionButton {
                        text: NacreI18n.tr("welcome.welcome-view.text.quickshell-docs", "Quickshell docs")
                        onClicked: NacreWelcomeApp.link("quickshell")
                    }
                    ActionButton {
                        text: NacreI18n.tr("welcome.welcome-view.text.cachyos-docs", "CachyOS docs")
                        onClicked: NacreWelcomeApp.link("cachyos")
                    }
                }
                NacreText {
                    width: parent.width
                    text: NacreI18n.tr("welcome.welcome-view.text.nacre-is-a-desktop-for-hyprland-built-with-quickshell-s", "Nacre is a desktop for Hyprland, built with Quickshell. System packages stay with your distribution; personal configuration stays on this computer.")
                    color: NacreTokens.mutedInk
                    font.pointSize: 10.5
                    wrapMode: Text.Wrap
                }
            }
        }
    }
    Item {
        id: footer
        objectName: "welcomeFooter"
        width: parent.width
        height: 64
        y: parent.height - height
        Rectangle {
            width: parent.width
            height: 1
            color: NacreTokens.outline
            opacity: .3
        }
        ActionButton {
            objectName: "welcomeBack"
            anchors.verticalCenter: parent.verticalCenter
            text: NacreWelcomeApp.step === 0 && NacreWelcomeApp.page !== "help" ? NacreI18n.tr("welcome.welcome-view.text.skip-tour", "Skip tour") : NacreI18n.tr("welcome.welcome-view.text.back", "Back")
            compact: true
            onClicked: NacreWelcomeApp.back()
        }
        Row {
            anchors.centerIn: parent
            spacing: 10
            Repeater {
                model: NacreWelcomeApp.steps
                NacreSurface {
                    required property string modelData
                    required property int index
                    objectName: "welcomeStep_" + modelData
                    width: 10
                    height: 10
                    radius: 5
                    color: NacreWelcomeApp.page === modelData ? NacreTokens.accent : NacreTokens.outline
                    NacreInteraction {
                        accessibleName: NacreI18n.tr("ui.welcome-view.accessibleName.step-value0-value1", "Step {value0}: {value1}", {
                            value0: (index + 1),
                            value1: modelData
                        })
                        function onClicked() {
                            NacreWelcomeApp.jump(modelData);
                        }
                    }
                }
            }
        }
        ActionButton {
            objectName: "welcomeDone"
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            selected: true
            text: NacreWelcomeApp.page === "ready" ? NacreI18n.tr("welcome.welcome-view.text.start-using-nacre", "Start using Nacre") : NacreWelcomeApp.page === "help" ? NacreI18n.tr("welcome.welcome-view.text.back-to-finish", "Back to finish") : NacreI18n.tr("welcome.welcome-view.text.next", "Next")
            enabled: !NacreWelcomeApp.demoBusy
            onClicked: NacreWelcomeApp.advance()
        }
    }
}
