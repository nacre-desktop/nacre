import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import qs.widgets
import qs.services

Item {
    id: root
    property bool detached: false
    required property PersistentProperties visibilities
    readonly property string kind: NacreWallpapers.preferences.kind || "static"
    readonly property string layout: NacreWallpapers.preferences.layout || "carousel"
    readonly property bool fullScreen: layout !== "carousel"
    readonly property bool motionEnabled: visibilities.launcher && kind === "dynamic" && !NacreWallpapers.preferences.paused && !WallpaperPlayback.locked && !WallpaperPlayback.sleeping
    readonly property var entries: NacreWallpapers.list.filter(entry => entry.dynamic === (kind === "dynamic") && query.text.trim().toLowerCase().split(/\s+/).every(word => (entry.name + " " + entry.path).toLowerCase().includes(word)))
    readonly property int count: entries.length
    property int currentIndex: 0
    readonly property var currentEntry: entries[currentIndex] || null
    property string selectionPath: ""
    property bool userNavigated: false
    property bool animateTravel: false
    property real travelTarget: 0
    property real travel: travelTarget
    property real wheelDelta: 0
    property real viewportWidth: Quickshell.screens[0].width
    property real viewportHeight: Quickshell.screens[0].height
    implicitWidth: Math.max(320, Math.min(1360, viewportWidth - 96))
    implicitHeight: fullScreen ? Math.max(360, viewportHeight - 100) : 320
    function reconcile() {
        animateTravel = false;
        travelAnimation.complete();
        const found = entries.findIndex(entry => entry.path === selectionPath);
        currentIndex = found >= 0 ? found : Math.max(0, Math.min(currentIndex, entries.length - 1));
        travelTarget = currentIndex;
    }
    function select(index) {
        if (!count)
            return;
        const next = (index % count + count) % count;
        const distance = ((next - currentIndex + count + Math.floor(count / 2)) % count) - Math.floor(count / 2);
        userNavigated = true;
        animateTravel = true;
        travelTarget += distance;
        currentIndex = next;
        selectionPath = currentEntry.path;
        if (!visibilities.previewOnly)
            NacreWallpapers.browse(selectionPath);
    }
    function wheelStep(event) {
        const pixels = event.pixelDelta.y || event.pixelDelta.x;
        const angles = event.angleDelta.y || event.angleDelta.x;
        const amount = pixels ? pixels * 2 : angles;
        if (!amount)
            return;
        wheelDelta += amount;
        const steps = Math.trunc(wheelDelta / 120);
        if (steps) {
            move(-steps);
            wheelDelta -= steps * 120;
        }
        event.accepted = true;
    }
    function circularOffset(index) {
        return count ? ((index - travel + count / 2) % count + count) % count - count / 2 : 0;
    }
    function move(delta) {
        select(currentIndex + delta);
    }
    function choose() {
        if (currentEntry && !visibilities.previewOnly) {
            selectionPath = currentEntry.path;
            NacreWallpapers.setWallpaper(selectionPath);
        }
    }
    onEntriesChanged: reconcile()
    Component.onCompleted: {
        selectionPath = NacreWallpapers.current;
        reconcile();
        query.forceActiveFocus();
    }
    Connections {
        target: root.visibilities
        function onLauncherChanged() {
            if (root.visibilities.launcher) {
                root.userNavigated = false;
                root.selectionPath = NacreWallpapers.current;
                root.reconcile();
            } else if (!root.visibilities.previewOnly)
                NacreWallpapers.commitSelection();
        }
    }
    Connections {
        target: NacreWallpapers
        function onCurrentChanged() {
            if (!root.userNavigated) {
                root.selectionPath = NacreWallpapers.current;
                root.reconcile();
            }
        }
    }
    Behavior on travel {
        enabled: root.animateTravel && root.visibilities.launcher && NacreTokens.motionEnabled
        NumberAnimation {
            id: travelAnimation
            duration: NacreTokens.motionEnabled ? 300 : 0
            easing.type: Easing.OutCubic
        }
    }
    NacreWallpaperBackdrop {
        objectName: "galleryBackdrop"
        anchors.fill: parent
        visible: root.fullScreen
        path: root.fullScreen ? root.currentEntry?.preview || root.currentEntry?.poster || "" : ""
    }
    Rectangle {
        objectName: "galleryScrim"
        anchors.fill: parent
        color: "#70000000"
        visible: root.fullScreen
    }
    MouseArea {
        anchors.fill: parent
        onClicked: if (!root.detached)
            root.visibilities.launcher = false
    }
    Column {
        id: toolbar
        anchors.top: parent.top
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.topMargin: root.fullScreen ? 18 : 12
        spacing: 8
        GridLayout {
            anchors.horizontalCenter: parent.horizontalCenter
            columns: root.width < 720 ? 1 : 5
            columnSpacing: 12
            rowSpacing: 8
            Row {
                objectName: "wallpaperMediaChoices"
                Layout.alignment: Qt.AlignHCenter
                spacing: 8
                ActionButton {
                    text: NacreI18n.tr("launcher.wallpaper-picker.text.static", "Static")
                    compact: true
                    selected: root.kind === "static"
                    onClicked: NacreWallpapers.preference({
                        kind: "static"
                    })
                }
                ActionButton {
                    text: NacreI18n.tr("launcher.wallpaper-picker.text.dynamic", "Dynamic")
                    compact: true
                    selected: root.kind === "dynamic"
                    onClicked: NacreWallpapers.preference({
                        kind: "dynamic"
                    })
                }
            }
            Rectangle {
                objectName: "wallpaperModeDivider"
                Layout.alignment: Qt.AlignHCenter
                Layout.preferredWidth: root.width < 720 ? 160 : 1
                Layout.preferredHeight: root.width < 720 ? 1 : 24
                color: NacreTokens.outline
            }
            Row {
                objectName: "wallpaperLayoutChoices"
                Layout.alignment: Qt.AlignHCenter
                spacing: 8
                ActionButton {
                    text: NacreI18n.tr("launcher.wallpaper-picker.text.carousel", "Carousel")
                    compact: true
                    selected: root.layout === "carousel"
                    onClicked: NacreWallpapers.preference({
                        layout: "carousel"
                    })
                }
                ActionButton {
                    text: NacreI18n.tr("launcher.wallpaper-picker.text.spotlight", "Spotlight")
                    compact: true
                    selected: root.layout === "spotlight"
                    onClicked: NacreWallpapers.preference({
                        layout: "spotlight"
                    })
                }
                ActionButton {
                    text: NacreI18n.tr("launcher.wallpaper-picker.text.hexagons", "Hexagons")
                    compact: true
                    selected: root.layout === "hexagons"
                    onClicked: NacreWallpapers.preference({
                        layout: "hexagons"
                    })
                }
            }
            Rectangle {
                Layout.alignment: Qt.AlignHCenter
                Layout.preferredWidth: 1
                Layout.preferredHeight: 24
                visible: root.width >= 720
                color: NacreTokens.outline
            }
            ActionButton {
                text: NacreI18n.tr("launcher.wallpaper-picker.text.add", "Add")
                icon: "add"
                compact: true
                onClicked: NacreWallpapers.pickFiles()
            }
        }
        NacreTextField {
            id: query
            anchors.horizontalCenter: parent.horizontalCenter
            objectName: "wallpaperSearch"
            width: Math.max(160, Math.min(640, root.width - 48))
            height: 40
            padding: 12
            placeholderText: NacreI18n.tr("launcher.wallpaper-picker.placeholderText.search-wallpapers", "Search wallpapers")
            onTextChanged: root.reconcile()
            Keys.onLeftPressed: root.move(-1)
            Keys.onRightPressed: root.move(1)
            Keys.onEscapePressed: {
                if (root.detached) {
                    query.text = "";
                } else
                    root.visibilities.launcher = false;
            }
            onAccepted: root.choose()
        }
    }
    NacreText {
        anchors.centerIn: parent
        visible: !root.count
        text: NacreWallpapers.loading ? NacreI18n.tr("launcher.wallpaper-picker.text.loading-wallpapers", "Loading wallpapers…") : NacreI18n.tr("launcher.wallpaper-picker.text.no-matching-wallpapers", "No matching wallpapers")
        color: root.fullScreen ? "white" : NacreColours.palette.m3onSurface
    }
    Item {
        id: carousel
        objectName: "carouselStrip"
        anchors.horizontalCenter: parent.horizontalCenter
        y: toolbar.y + toolbar.height + 10
        width: slots * cardWidth + Math.max(0, slots - 1) * 16
        height: Math.max(0, Math.min(root.height - y - 52, cardWidth * .66))
        clip: true
        visible: root.layout === "carousel"
        readonly property real availableWidth: Math.max(0, root.width - 48)
        readonly property int candidate: Math.min(root.count, Math.max(1, Math.round((availableWidth / 266 - 1) / 2) * 2 + 1))
        readonly property int slots: candidate > 1 && candidate % 2 === 0 ? candidate - 1 : candidate
        readonly property real cardWidth: Math.min(280, (availableWidth - Math.max(0, slots - 1) * 16) / Math.max(1, slots))
        Repeater {
            model: root.entries
            Card {
                required property int index
                required property var modelData
                objectName: "carouselCard" + index
                entry: modelData
                decode: Math.abs(root.circularOffset(index)) <= Math.max(2, Math.ceil(carousel.slots / 2) + 1) || Math.min(Math.abs(index - root.currentIndex), root.count - Math.abs(index - root.currentIndex)) <= Math.max(2, Math.ceil(carousel.slots / 2) + 1)
                width: carousel.cardWidth
                height: carousel.height
                x: (carousel.width - width) / 2 + ((((index - root.travel + root.count / 2) % root.count) + root.count) % root.count - root.count / 2) * (width + 16)
                selected: index === root.currentIndex
                visible: x + width > 0 && x < carousel.width
                onChosen: root.select(index)
            }
        }
        WheelHandler {
            target: null
            onWheel: event => root.wheelStep(event)
        }
    }
    Item {
        id: spotlight
        objectName: "spotlightStrip"
        x: 24
        readonly property real topInset: toolbar.y + toolbar.height + 24
        readonly property real availableHeight: Math.max(0, root.height - topInset - navigation.height - 40)
        y: topInset + (availableHeight - height) / 2
        width: Math.max(0, root.width - 48)
        height: heroWidth * 9 / 16
        clip: true
        visible: root.layout === "spotlight"
        // A coverflow deck keeps every preview widescreen. Side cards overlap
        // behind the hero instead of squeezing the collection into tall strips.
        readonly property real heroWidth: Math.min(width * (root.count > 1 ? .5 : .8), 1000, availableHeight * 16 / 9)
        readonly property int sideSlots: Math.max(0, Math.min(2, Math.ceil((root.count - 1) / 2)))
        readonly property real sideWidth: heroWidth * .62
        Repeater {
            model: root.entries
            Card {
                required property int index
                required property var modelData
                readonly property real offset: root.circularOffset(index)
                readonly property real weight: Math.max(0, 1 - Math.abs(offset))
                readonly property real distance: Math.abs(offset)
                readonly property real centerDistance: spotlight.heroWidth * .54
                objectName: "spotlightCard" + index
                entry: modelData
                decode: Math.abs(offset) <= spotlight.sideSlots + 1
                width: spotlight.sideWidth * Math.max(.7, 1 - Math.max(0, distance - 1) * .15) + (spotlight.heroWidth - spotlight.sideWidth) * weight
                height: width * 9 / 16
                x: spotlight.width / 2 + Math.sign(offset) * (Math.min(1, distance) * centerDistance + Math.max(0, distance - 1) * spotlight.heroWidth * .2) - width / 2
                y: (spotlight.height - height) / 2
                z: 100 - distance
                opacity: Math.max(0, Math.min(1, spotlight.sideSlots + 1 - distance)) * Math.max(.5, 1 - distance * .2)
                selected: index === root.currentIndex
                visible: opacity > 0 && x + width > 0 && x < spotlight.width
                onChosen: root.select(index)
            }
        }
        WheelHandler {
            target: null
            onWheel: event => root.wheelStep(event)
        }
    }
    Flickable {
        id: hexagons
        x: 24
        y: toolbar.y + toolbar.height + 22
        width: root.width - 48
        height: root.height - y - 56
        visible: root.layout === "hexagons"
        clip: true
        readonly property int columns: Math.max(3, Math.min(7, Math.floor(width / 230)))
        readonly property real tileWidth: Math.min(340, (width - 30) / (1 + (columns - 1) * .76))
        readonly property real tileHeight: tileWidth * .87
        contentWidth: width
        contentHeight: root.count ? Math.ceil(root.count / columns) * (tileHeight + 12) + tileHeight * .5 : 0
        Repeater {
            model: root.entries
            NacreWallpaperHex {
                required property var modelData
                required property int index
                entry: modelData
                width: hexagons.tileWidth
                height: hexagons.tileHeight
                x: (hexagons.width - (width + (hexagons.columns - 1) * width * .76)) / 2 + (index % hexagons.columns) * width * .76
                y: Math.floor(index / hexagons.columns) * (height + 12) + (index % hexagons.columns % 2) * height * .5
                visible: y + height >= hexagons.contentY - height && y <= hexagons.contentY + hexagons.height + height
                selected: index === root.currentIndex
                previewMotion: selected && root.motionEnabled
                onChosen: root.select(index)
            }
        }
        FastScroll {
            view: hexagons
        }
    }
    Row {
        id: navigation
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: root.layout === "spotlight" ? Math.max(12, root.height - spotlight.y - spotlight.height - 16 - height) : 12
        spacing: 14
        ActionButton {
            text: ""
            icon: "chevron_left"
            compact: true
            enabled: root.count > 0
            onClicked: root.move(-1)
        }
        NacreText {
            anchors.verticalCenter: parent.verticalCenter
            width: Math.min(420, root.width - 300)
            horizontalAlignment: Text.AlignHCenter
            elide: Text.ElideRight
            text: root.currentEntry?.name || ""
            font.pointSize: 11
            color: root.fullScreen ? "white" : NacreColours.palette.m3onSurface
        }
        ActionButton {
            text: ""
            icon: "chevron_right"
            compact: true
            enabled: root.count > 0
            onClicked: root.move(1)
        }
    }
    Keys.onEscapePressed: {
        if (root.detached) {
            query.text = "";
        } else
            root.visibilities.launcher = false;
    }
    Keys.onLeftPressed: root.move(-1)
    Keys.onRightPressed: root.move(1)
    Keys.onReturnPressed: root.choose()
    DropArea {
        anchors.fill: parent
        onDropped: drop => {
            if (drop.urls.length)
                NacreWallpapers.addFiles(drop.urls.map(String));
        }
    }
    component Card: NacreSurface {
        id: card
        required property var entry
        property bool selected: false
        property bool decode: true
        signal chosen
        radius: 15
        color: NacreColours.palette.m3surfaceContainer
        border.width: selected ? 2 : 0
        border.color: NacreColours.palette.m3primary
        NacreClip {
            anchors.fill: parent
            anchors.margins: 3
            radius: 12
            Image {
                id: image
                anchors.fill: parent
                source: card.decode ? "file://" + (card.entry.thumbnail || card.entry.poster) : ""
                fillMode: Image.PreserveAspectCrop
                asynchronous: true
                cache: true
                retainWhileLoading: true
                sourceSize.width: 640
                sourceSize.height: 420
            }
            Image {
                anchors.fill: image
                visible: card.selected && root.fullScreen && status === Image.Ready
                source: card.selected && root.fullScreen ? "file://" + (card.entry.preview || card.entry.poster) : ""
                fillMode: Image.PreserveAspectCrop
                asynchronous: true
                retainWhileLoading: true
                sourceSize.width: 1600
                sourceSize.height: 1000
            }
            NacreWallpaperMotion {
                anchors.fill: image
                entry: card.entry
                running: card.selected && root.motionEnabled
            }
        }
        MouseArea {
            anchors.fill: parent
            cursorShape: Qt.PointingHandCursor
            onClicked: card.chosen()
        }
    }
}
