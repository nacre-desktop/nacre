import QtQuick
import QtQuick.Dialogs
import QtQuick.Controls
import Quickshell
import qs.widgets
import qs.services

Item {
    id: root
    focus: true
    property string page: "studio"
    property string query: ""
    property string vision: "normal"
    property bool fineTune: false
    property var inspected: null
    readonly property var candidate: NacreColorsApp.preview
    readonly property var palette: candidate.palette?.colours || {}
    readonly property var audit: candidate.palette?.accessibility || {}
    readonly property int railWidth: width < 980 ? 180 : 210
    readonly property bool ready: NacreColorsApp.ready
    readonly property bool active: NacreColorsApp.visible
    function captureState() {
        return {
            page: page,
            query: query,
            vision: vision,
            fineTune: fineTune,
            inspected: inspected,
            scroll: canvas.contentY,
            entries: entries.contentY
        };
    }
    function restoreState(data) {
        page = data.page || "studio";
        query = data.query || "";
        vision = data.vision || "normal";
        fineTune = data.fineTune === true;
        inspected = data.inspected || null;
        Qt.callLater(() => {
            canvas.contentY = data.scroll || 0;
            entries.contentY = data.entries || 0;
        });
    }
    function search() {
        searchField.forceActiveFocus();
        searchField.selectAll();
    }
    function choosePath(value) {
        const text = value.toString();
        if (text.startsWith("file://"))
            NacreColorsApp.choose(decodeURIComponent(text.slice(7)));
    }
    function openImage() {
        fileDialog.open();
    }
    onPageChanged: {
        canvas.contentY = 0;
        NacreColorsApp.change({
            compare: page === "compare"
        });
    }
    Keys.onEscapePressed: query ? query = "" : NacreColorsApp.close()
    FileDialog {
        id: fileDialog
        title: "Choose a wallpaper"
        nameFilters: ["Wallpapers (*.png *.jpg *.jpeg *.webp *.gif *.mp4 *.webm *.mkv)", "All files (*)"]
        onAccepted: root.choosePath(selectedFile)
    }
    onVisibleChanged: if (!visible)
        fileDialog.close()
    DropArea {
        anchors.fill: parent
        onDropped: drop => {
            if (drop.urls.length)
                root.choosePath(drop.urls[0]);
        }
    }
    NacreSurface {
        id: rail
        y: 0
        width: root.railWidth
        height: Math.max(0, footer.y - y - 12)
        radius: 18
        color: NacreTokens.raised
        NacreTextField {
            id: searchField
            x: 12
            y: 12
            width: parent.width - 24
            placeholderText: "Find a wallpaper"
            text: root.query
            onTextEdited: root.query = text
        }
        ActionButton {
            x: 12
            y: 62
            width: parent.width - 24
            objectName: "openImageButton"
            text: "Open image"
            icon: "folder_open"
            compact: true
            onClicked: fileDialog.open()
        }
        ListView {
            id: entries
            x: 8
            y: 106
            width: parent.width - 16
            height: Math.max(0, parent.height - y - 8)
            clip: true
            spacing: 6
            model: NacreColorsApp.library.filter(w => !root.query || w.name.toLowerCase().includes(root.query.toLowerCase()))
            delegate: NacreSurface {
                id: entry
                required property var modelData
                width: entries.width
                height: 88
                readonly property bool selected: NacreColorsApp.options.image === modelData.path
                radius: 10
                color: selected ? Qt.alpha(NacreTokens.accent, .14) : "transparent"
                border.width: selected ? 1 : 0
                border.color: Qt.alpha(NacreTokens.accent, .65)
                NacreImage {
                    x: 8
                    y: 8
                    width: 60
                    height: 42
                    path: entry.modelData.thumbnail
                    active: root.active
                    fillMode: Image.PreserveAspectCrop
                    asynchronous: true
                    retainWhileLoading: true
                }
                NacreText {
                    x: 77
                    y: 10
                    width: parent.width - x - 10
                    text: entry.modelData.name
                    wrapMode: Text.Wrap
                    maximumLineCount: 3
                    elide: Text.ElideRight
                    font.pointSize: 10
                }
                NacreText {
                    x: 8
                    y: 62
                    width: parent.width - 16
                    text: entry.modelData.dynamic ? "Animated" : "Static"
                    font.pointSize: 9
                    color: NacreTokens.mutedInk
                }
                NacreInteraction {
                    accessibleName: entry.modelData.name
                    function onClicked() {
                        NacreColorsApp.choose(entry.modelData.path);
                    }
                }
            }
            FastScroll {
                view: entries
            }
            NacreText {
                anchors.centerIn: parent
                width: parent.width - 16
                visible: entries.count === 0
                text: "No matching wallpapers. Open an image to explore it."
                wrapMode: Text.Wrap
                font.pointSize: 10
                color: NacreTokens.mutedInk
            }
        }
    }
    Flickable {
        id: canvas
        objectName: "colorsCanvas"
        x: rail.width + 16
        y: 0
        width: parent.width - x
        height: rail.height
        contentWidth: width
        contentHeight: body.implicitHeight
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        FastScroll {
            view: canvas
        }
        ScrollBar.vertical: NacreScrollBar {}
        Column {
            id: body
            width: parent.width
            spacing: 14
            NacreText {
                width: parent.width
                text: root.candidate.name || "Explore a wallpaper"
                font.pointSize: 20
                wrapMode: Text.Wrap
            }
            NacreText {
                text: NacreColorsApp.previewBusy ? "Preparing preview…" : root.candidate.frames > 1 ? "Animated" : "Click the image to pick a color"
                font.pointSize: 10
                color: NacreTokens.mutedInk
            }
            Flow {
                id: studio
                width: parent.width
                spacing: 20
                visible: root.page === "studio"
                readonly property bool split: width >= 720
                Column {
                    width: studio.split ? (studio.width - 20) * .58 : studio.width
                    spacing: 12
                    Item {
                        id: photo
                        width: parent.width
                        height: Math.min(460, width * .56)
                        NacreImage {
                            id: wallpaper
                            anchors.fill: parent
                            path: root.candidate.thumbnail || ""
                            active: root.active && studio.visible
                            fillMode: Image.PreserveAspectFit
                            asynchronous: true
                            cache: true
                            retainWhileLoading: true
                            requestedSize: Qt.size(960, 600)
                        }
                        Repeater {
                            model: !root.candidate.favorite && root.inspected?.regions ? root.inspected.regions.cells : []
                            Rectangle {
                                required property var modelData
                                readonly property var regions: root.inspected.regions
                                width: wallpaper.paintedWidth / regions.columns
                                height: wallpaper.paintedHeight / regions.rows
                                x: (photo.width - wallpaper.paintedWidth) / 2 + (modelData.index % regions.columns) * width
                                y: (photo.height - wallpaper.paintedHeight) / 2 + Math.floor(modelData.index / regions.columns) * height
                                color: Qt.alpha("#" + root.inspected.hex, .30)
                                border.width: 1
                                border.color: "#" + root.inspected.hex
                            }
                        }
                        MouseArea {
                            anchors.fill: parent
                            onClicked: event => {
                                const x = (event.x - (photo.width - wallpaper.paintedWidth) / 2) / wallpaper.paintedWidth;
                                const y = (event.y - (photo.height - wallpaper.paintedHeight) / 2) / wallpaper.paintedHeight;
                                if (x >= 0 && x <= 1 && y >= 0 && y <= 1)
                                    NacreColorsApp.change({
                                        pick: {
                                            x: x,
                                            y: y
                                        }
                                    });
                            }
                        }
                    }

                    NacreText {
                        text: "Color families in the image"
                        font.pointSize: 11
                    }
                    Flow {
                        width: parent.width
                        spacing: 7
                        Repeater {
                            model: root.candidate.imageColors || root.candidate.palette?.source?.clusters?.slice(0, 8) || []
                            NacreColorSample {
                                required property var modelData
                                entry: modelData
                                width: Math.max(44, Math.floor((parent.width - 49) / 8))
                                height: 60
                                onPicked: value => NacreColorsApp.change({
                                        accent: value
                                    })
                                onInspect: entry => root.inspected = entry
                                onLeave: root.inspected = null
                            }
                        }
                    }
                    NacreText {
                        text: "Other shades: " + Math.round((root.candidate.otherCoverage || 0) * 100) + "%"
                        font.pointSize: 9
                        color: NacreTokens.mutedInk
                        visible: (root.candidate.otherCoverage || 0) > .005
                    }
                    NacreText {
                        width: parent.width
                        wrapMode: Text.Wrap
                        font.pointSize: 9
                        color: NacreTokens.mutedInk
                        text: "Similar shades are grouped. Percentages show image area; unlisted colors are Other."
                    }
                    NacreText {
                        text: "Color style"
                        font.pointSize: 12
                    }
                    Flow {
                        id: toolbar
                        objectName: "colorStyleControls"
                        width: parent.width
                        spacing: 6
                        Repeater {
                            model: ["natural", "pigment", "harmony", "pop", "mist", "vivid", "pearl"]
                            ActionButton {
                                required property string modelData
                                objectName: modelData + "Personality"
                                compact: true
                                text: modelData.charAt(0).toUpperCase() + modelData.slice(1)
                                selected: !NacreColorsApp.options.favoriteId && NacreColorsApp.options.personality === modelData
                                onClicked: NacreColorsApp.change({
                                    personality: modelData
                                })
                            }
                        }
                    }
                    NacreText {
                        width: parent.width
                        wrapMode: Text.Wrap
                        color: NacreTokens.mutedInk
                        font.pointSize: 10
                        text: ({
                                natural: "The original Nacre balance of the wallpaper's colors",
                                pigment: "Puts more weight on the subject and small details",
                                harmony: "Related hues for a quieter theme",
                                pop: "A contrasting detail becomes the main accent",
                                mist: "Soft colors for focus",
                                vivid: "Stronger wallpaper colors",
                                pearl: "Nacre's mother-of-pearl signature"
                            })[NacreColorsApp.options.personality] || "Time-of-day tint"
                    }
                }
                Column {
                    width: studio.split ? (studio.width - 20) * .42 : studio.width
                    spacing: 12
                    NacreText {
                        text: "Desktop preview"
                        font.pointSize: 12
                    }
                    NacreThemePreview {
                        active: root.active && studio.visible
                        width: parent.width
                        height: Math.min(280, width * .75)
                        palette: root.palette
                        wallpaper: root.candidate.thumbnail || ""
                        workspaceColors: NacreColorsApp.options.workspaceColors === true
                    }
                    NacreText {
                        text: root.audit.textPasses ? "✓ Readable" : "Check readability in Accessibility"
                        color: NacreTokens.mutedInk
                        font.pointSize: 10
                    }
                    Column {
                        width: parent.width
                        spacing: 6
                        Repeater {
                            model: root.candidate.palette ? ["primary", "secondary", "tertiary"] : []
                            NacreAccentTarget {
                                required property string modelData
                                role: modelData
                                width: parent.width
                                value: root.palette[modelData] || "808080"
                                pinned: !!NacreColorsApp.options.overrides?.[modelData]
                                onAssign: value => NacreColorsApp.setRole(modelData, value)
                                onClear: NacreColorsApp.setRole(modelData, "")
                            }
                        }
                    }
                    ActionButton {
                        text: root.fineTune ? "Hide fine-tuning" : "Fine-tune"
                        icon: "tune"
                        compact: true
                        onClicked: root.fineTune = !root.fineTune
                    }
                    Column {
                        width: parent.width
                        spacing: 10
                        visible: root.fineTune
                        NacreText {
                            text: "Background tint"
                            font.pointSize: 11
                        }
                        Row {
                            spacing: 6
                            ActionButton {
                                text: "Accent"
                                compact: true
                                selected: !NacreColorsApp.options.backgroundFromWallpaper
                                onClicked: NacreColorsApp.change({
                                    backgroundFromWallpaper: false
                                })
                            }
                            ActionButton {
                                objectName: "wallpaperBackgroundSwitch"
                                text: "Wallpaper"
                                compact: true
                                selected: NacreColorsApp.options.backgroundFromWallpaper
                                enabled: NacreColorsApp.options.personality !== "pearl"
                                onClicked: NacreColorsApp.change({
                                    backgroundFromWallpaper: true
                                })
                            }
                        }
                        NacreText {
                            width: parent.width
                            wrapMode: Text.Wrap
                            color: NacreTokens.mutedInk
                            font.pointSize: 9
                            text: NacreColorsApp.options.backgroundFromWallpaper ? "Panels use shadow tones in dark mode and bright tones in light mode. Accent colors stay the same." : "Panels use a restrained tint of the main accent. This does not change the wallpaper image."
                        }
                        NacreText {
                            text: "Brightness"
                            font.pointSize: 10
                        }
                        NacreAdjustSlider {
                            width: parent.width
                            from: -.15
                            to: .15
                            value: NacreColorsApp.options.brightness || 0
                            onMoved: NacreColorsApp.change({
                                brightness: value
                            })
                        }
                        NacreText {
                            text: "Color strength"
                            font.pointSize: 10
                        }
                        NacreAdjustSlider {
                            width: parent.width
                            from: 0
                            to: 1.6
                            value: NacreColorsApp.options.saturation ?? 1
                            onMoved: NacreColorsApp.change({
                                saturation: value
                            })
                        }
                        ActionButton {
                            text: "Shift with time of day"
                            compact: true
                            selected: NacreColorsApp.options.timeTint === true
                            onClicked: NacreColorsApp.change({
                                timeTint: !NacreColorsApp.options.timeTint,
                                tideAutomatic: !NacreColorsApp.options.timeTint
                            })
                        }
                        ActionButton {
                            text: "Color workspace chips"
                            compact: true
                            selected: NacreColorsApp.options.workspaceColors === true
                            onClicked: NacreColorsApp.change({
                                workspaceColors: !NacreColorsApp.options.workspaceColors
                            })
                        }
                        ActionButton {
                            text: "Reset adjustments"
                            compact: true
                            onClicked: NacreColorsApp.change({
                                overrides: {},
                                brightness: 0,
                                saturation: 1,
                                accent: null
                            })
                        }
                    }
                }
            }
            Column {
                visible: root.page === "compare"
                width: parent.width
                spacing: 14
                NacreText {
                    text: "Compare styles"
                    font.pointSize: 14
                }
                Flow {
                    id: comparisons
                    width: parent.width
                    spacing: 12
                    Repeater {
                        model: root.candidate.comparisons || []
                        Column {
                            required property var modelData
                            width: Math.max(300, (comparisons.width - 12) / 2)
                            spacing: 8
                            NacreText {
                                text: modelData.personality.replace(/^./, s => s.toUpperCase())
                                font.pointSize: 11
                            }
                            NacreThemePreview {
                                active: root.active && root.page === "compare"
                                width: parent.width
                                height: Math.min(260, width * .60)
                                palette: modelData.colours
                                wallpaper: root.candidate.thumbnail || ""
                            }
                        }
                    }
                }
            }
            Column {
                visible: root.page === "accessibility"
                width: parent.width
                spacing: 12
                NacreText {
                    text: "Readability and color vision"
                    font.pointSize: 16
                }
                NacreText {
                    width: parent.width
                    wrapMode: Text.Wrap
                    text: "Minimum audited text contrast: " + Math.round((root.audit.minimumTextContrast || 0) * 100) / 100 + ":1. Coverage is measured image area; salience weights central and detailed areas separately."
                    font.pointSize: 10
                    color: NacreTokens.mutedInk
                }
                Flow {
                    width: parent.width
                    spacing: 6
                    Repeater {
                        model: ["normal", "protan", "deutan", "tritan"]
                        ActionButton {
                            required property string modelData
                            text: modelData
                            compact: true
                            selected: root.vision === modelData
                            onClicked: root.vision = modelData
                        }
                    }
                }
                NacreThemePreview {
                    active: root.active && root.page === "accessibility"
                    width: Math.min(parent.width, 620)
                    height: 300
                    palette: root.vision === "normal" ? root.palette : root.audit.simulated?.[root.vision] || root.palette
                    wallpaper: root.candidate.thumbnail || ""
                }
                NacreText {
                    width: parent.width
                    wrapMode: Text.Wrap
                    text: (root.audit.warnings || []).join("\n")
                    font.pointSize: 10
                    color: NacreTokens.mutedInk
                }
                Repeater {
                    model: root.audit.text || []
                    NacreText {
                        required property var modelData
                        width: parent.width
                        font.pointSize: 9
                        text: modelData.foreground + " on " + modelData.background + " — " + modelData.ratio + ":1"
                        color: NacreTokens.mutedInk
                    }
                }
            }
        }
    }
    Item {
        id: footer
        y: parent.height - height
        width: parent.width
        height: 80
        NacreText {
            width: parent.width - 130
            text: NacreColorsApp.error || NacreColorsApp.status || "Preview only — Apply when you're ready"
            font.pointSize: 10
            color: NacreColorsApp.error ? NacreColours.palette.m3error : NacreTokens.mutedInk
            wrapMode: Text.Wrap
        }
        Row {
            y: 30
            spacing: 8
            ActionButton {
                objectName: "exportThemes"
                text: "Export"
                icon: "upload"
                compact: true
                enabled: root.ready && !NacreColorsApp.actionBusy
                onClicked: NacreColorsApp.action("export")
            }
        }
        ActionButton {
            objectName: "applyPalette"
            anchors.right: parent.right
            y: 30
            text: NacreColorsApp.applied ? "In use" : "Apply"
            icon: "check"
            selected: true
            enabled: root.ready && !NacreColorsApp.actionBusy && !NacreColorsApp.applied
            onClicked: NacreColorsApp.action("apply")
        }
    }
}
