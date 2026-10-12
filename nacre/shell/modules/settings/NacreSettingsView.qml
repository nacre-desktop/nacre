pragma ComponentBehavior: Bound
import QtQuick
import Quickshell
import qs.widgets
import qs.services
import "pages"
import "settings-catalog.js" as Catalog

Item {
    id: root
    signal closeRequested
    focus: true
    function captureState() {
        return {
            page: page,
            query: query,
            scroll: scroll.contentY,
            categories: categories.contentY
        };
    }
    function restoreState(data) {
        query = data.page === page ? data.query || "" : "";
        Qt.callLater(() => {
            scroll.contentY = data.page === page ? data.scroll || 0 : 0;
            categories.contentY = data.categories || 0;
        });
    }
    function focusContent() {
        forceActiveFocus(Qt.OtherFocusReason);
    }
    function focusSearch() {
        search.forceActiveFocus(Qt.ShortcutFocusReason);
        search.selectAll();
    }
    Keys.onEscapePressed: {
        if (query) {
            query = "";
        } else
            closeRequested();
    }
    property bool active: true
    property string page: NacrePanelState.settingsPage
    property string query: ""
    readonly property var pages: Catalog.localize(Catalog.pages, NacreI18n.tr)
    readonly property var current: Catalog.find(page, NacreI18n.tr)
    readonly property var matches: Catalog.search(query, NacreI18n.tr)
    readonly property bool narrow: width < 760
    readonly property real navigationLabelWidth: Math.max(120, ...pages.map(row => Math.ceil(navigationMetrics.advanceWidth(row.label))))
    implicitWidth: 1120
    implicitHeight: 740
    FontMetrics {
        id: navigationMetrics
        font.family: NacreTokens.textFamily
        font.pointSize: 10
    }
    FontMetrics {
        id: searchMetrics
        font: search.font
    }
    function open(id) {
        const target = Catalog.find(id).id;
        page = target;
        query = "";
        if (NacrePanelState.settingsPage !== target)
            NacrePanelState.settingsPage = target;
        resetScroll();
    }
    function resetScroll() {
        if (scroll && pageMotion) {
            pageMotion.cancel();
            scroll.contentY = 0;
        }
    }
    onPageChanged: resetScroll()
    onQueryChanged: resetScroll()
    Connections {
        target: NacrePanelState
        function onSettingsPageChanged() {
            root.open(NacrePanelState.settingsPage);
        }
    }
    NacreSurface {
        id: rail
        x: 0
        y: 0
        width: root.narrow ? root.width : Math.max(208, root.navigationLabelWidth + 68, Math.ceil(searchMetrics.advanceWidth(search.placeholderText)) + 52)
        height: root.narrow ? 118 : root.height
        radius: 18
        color: NacreColours.palette.m3surfaceContainer
        NacreTextField {
            id: search
            objectName: "settingsSearch"
            x: 12
            y: 12
            width: parent.width - 24
            height: 40
            padding: 10
            text: root.query
            placeholderText: NacreI18n.tr("settings.settings-view.placeholderText.search-settings", "Search settings")
            onTextEdited: root.query = text
            onAccepted: if (root.matches.length)
                root.open(root.matches[0].id)
        }
        ListView {
            id: categories
            objectName: "settingsCategories"
            x: 10
            y: 64
            width: parent.width - 20
            height: parent.height - 74
            orientation: root.narrow ? ListView.Horizontal : ListView.Vertical
            model: root.pages
            spacing: 4
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            delegate: NacreSurface {
                required property var modelData
                width: root.narrow ? Math.max(130, navigationMetrics.advanceWidth(modelData.label) + 56) : categories.width
                height: 38
                radius: 19
                color: root.page === modelData.id ? NacreColours.palette.m3secondaryContainer : "transparent"
                Row {
                    x: 12
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 9
                    NacreIcon {
                        text: parent.parent.modelData.icon
                        font.pointSize: 14
                    }
                    NacreText {
                        text: parent.parent.modelData.label
                        width: parent.parent.width - 56
                        font.pointSize: 10
                        elide: Text.ElideRight
                    }
                }
                NacreInteraction {
                    accessibleName: parent.modelData.label
                    function onClicked() {
                        root.open(parent.modelData.id);
                    }
                }
            }
            FastScroll {
                view: categories
            }
        }
    }
    Flickable {
        id: scroll
        objectName: "settingsScroll"
        x: root.narrow ? 0 : rail.width + 24
        y: root.narrow ? 130 : 0
        width: Math.max(0, parent.width - x)
        height: Math.max(0, parent.height - y)
        contentWidth: width
        contentHeight: body.implicitHeight
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        FastScroll {
            id: pageMotion
            view: scroll
        }
        Column {
            id: body
            width: parent.width
            spacing: 20
            NacreText {
                width: parent.width
                text: root.query.trim() ? NacreI18n.tr("settings.settings-view.text.search-results", "Search results") : root.current.label
                font.pointSize: 21
                color: NacreTokens.accent
                wrapMode: Text.Wrap
            }
            NacreText {
                width: parent.width
                text: root.query.trim() ? NacreI18n.tr("ui.settings-view.text.value0-matching-sections", "{value0} matching sections", {
                    value0: root.matches.length
                }) : root.current.detail
                font.pointSize: 11
                color: NacreTokens.mutedInk
                wrapMode: Text.Wrap
            }
            Column {
                width: parent.width
                spacing: 10
                visible: root.query.trim().length > 0
                Repeater {
                    model: root.query.trim() ? root.matches : []
                    delegate: NacreSurface {
                        required property var modelData
                        width: parent.width
                        height: 78
                        radius: 16
                        color: NacreColours.palette.m3surfaceContainer
                        Column {
                            x: 16
                            y: 14
                            width: parent.width - 32
                            spacing: 8
                            NacreText {
                                width: parent.width
                                text: parent.parent.modelData.label
                                font.pointSize: 13
                                elide: Text.ElideRight
                            }
                            NacreText {
                                width: parent.width
                                text: parent.parent.modelData.detail
                                font.pointSize: 10
                                color: NacreTokens.mutedInk
                                elide: Text.ElideRight
                            }
                        }
                        NacreInteraction {
                            accessibleName: parent.modelData.label
                            function onClicked() {
                                root.open(parent.modelData.id);
                            }
                        }
                    }
                }
            }
            Loader {
                id: loaded
                objectName: "settingsPage"
                width: parent.width
                active: root.active && !root.query.trim()
                sourceComponent: ({
                        appearance: appearance,
                        desktop: desktop,
                        displays: displays,
                        sound: sound,
                        network: network,
                        bluetooth: bluetooth,
                        notifications: notifications,
                        workflows: workflows,
                        lock: lock,
                        time: time,
                        ai: ai,
                        maintenance: maintenance
                    })[root.current.kind]
            }
        }
    }
    Component {
        id: appearance
        NacreAppearancePage {}
    }
    Component {
        id: desktop
        NacreDesktopPage {}
    }
    Component {
        id: displays
        NacreDisplaysPage {}
    }
    Component {
        id: sound
        NacreSoundPage {}
    }
    Component {
        id: network
        NacreNetworkPage {}
    }
    Component {
        id: bluetooth
        NacreBluetoothPage {}
    }
    Component {
        id: notifications
        NacreNotificationPage {}
    }
    Component {
        id: workflows
        NacreWorkflowsPage {}
    }
    Component {
        id: lock
        NacreLockPage {}
    }
    Component {
        id: time
        NacreTimePage {}
    }
    Component {
        id: ai
        NacreAiPage {}
    }
    Component {
        id: maintenance
        NacreMaintenancePage {}
    }
}
