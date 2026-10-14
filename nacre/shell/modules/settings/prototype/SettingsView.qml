pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import qs.widgets
import "../registry/registry.js" as Registry

FocusScope {
    id: root
    required property var registry
    required property var store
    property var capabilities: ({})
    property var screen
    property string page: "panels"
    property string query: ""
    property string highlight: ""
    property string feedback: ""
    property var pendingAction: null
    property int resultIndex: 0
    property bool overflowOpen: false
    readonly property var schema: registry.schema
    readonly property var current: schema.pages.find(item => item.id === page)
    readonly property bool narrow: width < 760
    readonly property var navigation: schema.pages.filter(item => !item.parent && Registry.visiblePage(schema, item, capabilities)).sort((a, b) => a.order - b.order)
    readonly property var sections: schema.sections.filter(item => item.page === page && Registry.allowed(item.visibleWhen, capabilities)).sort((a, b) => a.order - b.order)
    readonly property var results: Registry.search(schema, query, capabilities, registry.text)
    signal closeRequested
    objectName: "settingsPrototype"
    focus: true
    function focusSearch() {
        search.forceActiveFocus();
    }
    function open(link) {
        const route = Registry.resolve(schema, link, capabilities);
        if (!route)
            return false;
        page = route.page;
        highlight = route.setting;
        query = "";
        feedback = "";
        overflowOpen = false;
        scroll.contentY = 0;
        Qt.callLater(() => {
            if (highlight) {
                const row = findRow(body, highlight);
                if (row)
                    scroll.contentY = Math.max(0, Math.min(scroll.contentHeight - scroll.height, row.mapToItem(body, 0, 0).y - NacreTokens.space.md));
            }
            if (nav.currentIndex >= 0 && nav.currentIndex < 6)
                nav.positionViewAtBeginning();
            root.forceActiveFocus();
        });
        return true;
    }
    function findRow(item, name) {
        if (item.objectName === name)
            return item;
        for (const child of item.children || []) {
            const found = findRow(child, name);
            if (found)
                return found;
        }
        return null;
    }
    function activateResult() {
        const result = results[Math.max(0, Math.min(results.length - 1, resultIndex))];
        if (result)
            open(result.page + "#" + result.id);
    }
    function action(item) {
        if (item.route)
            open(item.route);
        else if (item.confirm)
            pendingAction = item;
        else
            store.actionRequested(item.adapter);
    }
    onQueryChanged: {
        resultIndex = 0;
        scroll.contentY = 0;
    }
    onCapabilitiesChanged: {
        if (current && !Registry.visiblePage(schema, current, capabilities))
            open("panels");
    }
    Keys.onPressed: event => {
        if (event.key === Qt.Key_Escape) {
            if (pendingAction)
                pendingAction = null;
            else if (overflowOpen)
                overflowOpen = false;
            else if (query)
                query = "";
            else
                closeRequested();
            event.accepted = true;
        } else if (event.text && event.text.trim() && !(event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier))) {
            focusSearch();
            query += event.text;
            search.cursorPosition = search.length;
            event.accepted = true;
        }
    }
    NacreSurface {
        id: rail
        width: root.narrow ? root.width : 244
        height: root.narrow ? 110 : root.height
        radius: NacreTokens.radii.medium
        color: NacreTokens.raised
        NacreTextField {
            id: search
            objectName: "registrySearch"
            x: NacreTokens.space.md
            y: NacreTokens.space.md
            width: parent.width - 2 * x
            height: 44
            padding: NacreTokens.space.md
            text: root.query
            placeholderText: root.registry.text("settings.registry.ui.search")
            onTextEdited: root.query = text
            onAccepted: root.activateResult()
            background: NacreSurface {
                radius: NacreTokens.radii.small
                color: NacreTokens.body
                border.width: 1
                border.color: search.activeFocus ? NacreTokens.accent : Qt.alpha(NacreTokens.outline, .5)
            }
            Keys.onPressed: event => {
                if ([Qt.Key_Down, Qt.Key_Up].includes(event.key) && root.query) {
                    root.resultIndex = Math.max(0, Math.min(root.results.length - 1, root.resultIndex + (event.key === Qt.Key_Down ? 1 : -1)));
                    event.accepted = true;
                } else if (event.key === Qt.Key_Escape) {
                    if (root.query)
                        root.query = "";
                    else
                        root.closeRequested();
                    event.accepted = true;
                }
            }
        }
        ComboBox {
            visible: root.narrow
            x: NacreTokens.space.md
            y: 64
            width: parent.width - 2 * x
            height: 36
            model: root.navigation.map(item => root.registry.text(item.labelId))
            background: NacreSurface {
                radius: NacreTokens.radii.small
                color: NacreTokens.body
                border.width: 1
                border.color: NacreTokens.outline
            }
            contentItem: NacreText {
                text: root.registry.text(root.navigation[parent.currentIndex]?.labelId)
                leftPadding: NacreTokens.space.md
                rightPadding: NacreTokens.space.huge
                verticalAlignment: Text.AlignVCenter
                elide: Text.ElideRight
                font.pointSize: NacreTokens.textSize.caption
            }
            indicator: NacreIcon {
                anchors.right: parent.right
                anchors.rightMargin: NacreTokens.space.sm
                anchors.verticalCenter: parent.verticalCenter
                text: "expand_more"
            }
            currentIndex: root.navigation.findIndex(item => item.id === (root.current?.parent || root.page))
            font.family: NacreTokens.textFamily
            Accessible.name: root.registry.text("settings.registry.ui.navigation")
            onActivated: index => root.open(root.navigation[index].id)
        }
        ListView {
            id: nav
            objectName: "registryNavigation"
            visible: !root.narrow
            x: NacreTokens.space.sm
            y: 72
            width: parent.width - 2 * x
            height: parent.height - y - NacreTokens.space.sm
            clip: true
            model: root.navigation
            currentIndex: root.navigation.findIndex(item => item.id === (root.current?.parent || root.page))
            boundsBehavior: Flickable.StopAtBounds
            spacing: NacreTokens.space.xs
            section.property: "group"
            section.delegate: NacreText {
                required property string section
                width: nav.width
                height: 34
                leftPadding: NacreTokens.space.md
                topPadding: NacreTokens.space.md
                text: root.registry.text(root.schema.groups.find(item => item.id === section).labelId)
                color: NacreTokens.mutedInk
                font.pointSize: NacreTokens.textSize.caption
            }
            delegate: NacreSurface {
                id: navItem
                required property var modelData
                required property int index
                width: nav.width
                height: Math.max(42, label.implicitHeight + NacreTokens.space.md)
                radius: NacreTokens.radii.small
                color: nav.currentIndex === index ? Qt.alpha(NacreTokens.accent, .18) : "transparent"
                NacreIcon {
                    x: NacreTokens.space.md
                    anchors.verticalCenter: parent.verticalCenter
                    text: navItem.modelData.icon
                }
                NacreText {
                    id: label
                    x: 42
                    width: parent.width - x - NacreTokens.space.sm
                    anchors.verticalCenter: parent.verticalCenter
                    text: root.registry.text(navItem.modelData.labelId)
                    wrapMode: Text.Wrap
                    font.pointSize: NacreTokens.textSize.caption
                }
                NacreInteraction {
                    accessibleName: label.text
                    function onClicked() {
                        root.open(navItem.modelData.id);
                    }
                    Keys.onPressed: event => {
                        if ([Qt.Key_Return, Qt.Key_Enter, Qt.Key_Space].includes(event.key)) {
                            root.open(navItem.modelData.id);
                            event.accepted = true;
                        } else if ([Qt.Key_Up, Qt.Key_Down, Qt.Key_Home, Qt.Key_End].includes(event.key)) {
                            const next = event.key === Qt.Key_Home ? 0 : event.key === Qt.Key_End ? nav.count - 1 : Math.max(0, Math.min(nav.count - 1, navItem.index + (event.key === Qt.Key_Down ? 1 : -1)));
                            nav.currentIndex = next;
                            nav.positionViewAtIndex(next, ListView.Contain);
                            Qt.callLater(() => nav.itemAtIndex(next)?.children.find(item => typeof item.activate === "function")?.forceActiveFocus());
                            event.accepted = true;
                        }
                    }
                }
            }
            ScrollBar.vertical: NacreScrollBar {}
            FastScroll {
                view: nav
            }
        }
    }
    Flickable {
        id: scroll
        objectName: "registryScroll"
        x: root.narrow ? 0 : rail.width + NacreTokens.space.xxl
        y: root.narrow ? rail.height + NacreTokens.space.xl : 0
        width: root.width - x
        height: root.height - y
        contentWidth: width
        contentHeight: body.implicitHeight + NacreTokens.space.xl
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: NacreScrollBar {}
        FastScroll {
            view: scroll
        }
        Column {
            id: body
            width: scroll.width
            spacing: NacreTokens.space.xl
            Row {
                visible: !!root.current?.parent && !root.query
                spacing: NacreTokens.space.sm
                ActionButton {
                    text: root.registry.text("settings.registry.ui.back")
                    icon: "arrow_back"
                    compact: true
                    onClicked: root.open(root.current.parent)
                }
                NacreText {
                    anchors.verticalCenter: parent.verticalCenter
                    text: Registry.breadcrumb(root.schema, root.page, root.registry.text)
                    color: NacreTokens.mutedInk
                    font.pointSize: NacreTokens.textSize.caption
                }
            }
            Row {
                width: parent.width
                spacing: NacreTokens.space.sm
                NacreText {
                    width: parent.width - overflow.width - parent.spacing
                    text: root.registry.text(root.query ? "settings.registry.ui.results" : root.current?.labelId)
                    font.pointSize: NacreTokens.textSize.headline
                    font.weight: Font.Medium
                    wrapMode: Text.Wrap
                }
                ActionButton {
                    id: overflow
                    objectName: "registryOverflow"
                    visible: !root.query
                    icon: "more_horiz"
                    text: ""
                    accessibleLabel: root.registry.text("settings.registry.ui.overflow")
                    compact: true
                    onClicked: root.overflowOpen = !root.overflowOpen
                }
            }
            NacreText {
                width: parent.width
                text: root.query ? root.registry.text("settings.registry.ui.count", {
                    count: root.results.length
                }) : root.registry.text(root.current?.descriptionId)
                color: NacreTokens.mutedInk
                wrapMode: Text.Wrap
                font.pointSize: NacreTokens.textSize.body
            }
            ActionButton {
                objectName: "registryReset"
                visible: root.overflowOpen && !root.query
                text: root.registry.text("settings.registry.ui.reset")
                icon: "restart_alt"
                onClicked: {
                    root.store.reset(root.page);
                    root.overflowOpen = false;
                }
            }
            NacreText {
                visible: root.feedback.length > 0 || root.store.error.length > 0
                width: parent.width
                text: root.store.error || root.feedback
                wrapMode: Text.Wrap
                color: NacreTokens.mutedInk
                font.pointSize: NacreTokens.textSize.caption
            }
            SettingsCard {
                visible: !!root.pendingAction
                width: parent.width
                title: root.registry.text("settings.registry.ui.history-confirm")
                Row {
                    spacing: NacreTokens.space.sm
                    ActionButton {
                        objectName: "registryConfirmAction"
                        text: root.registry.text("settings.registry.ui.confirm")
                        onClicked: {
                            root.store.actionRequested(root.pendingAction.adapter);
                            root.pendingAction = null;
                        }
                    }
                    ActionButton {
                        text: root.registry.text("settings.registry.ui.cancel")
                        onClicked: root.pendingAction = null
                    }
                }
            }
            Column {
                width: parent.width
                spacing: NacreTokens.space.sm
                visible: root.query.length > 0
                NacreText {
                    width: parent.width
                    visible: root.results.length === 0
                    text: root.registry.text("settings.registry.ui.empty")
                    wrapMode: Text.Wrap
                }
                Repeater {
                    model: root.query ? root.results : []
                    delegate: NacreSurface {
                        id: result
                        required property var modelData
                        required property int index
                        objectName: "result-" + modelData.id
                        width: body.width
                        height: resultLabel.height + trail.height + NacreTokens.space.huge
                        radius: NacreTokens.radii.medium
                        color: root.resultIndex === index ? Qt.tint(NacreTokens.raised, Qt.alpha(NacreTokens.accent, .12)) : NacreTokens.raised
                        NacreText {
                            id: resultLabel
                            x: NacreTokens.space.xl
                            y: NacreTokens.space.md
                            width: parent.width - 2 * x
                            text: result.modelData.label
                            wrapMode: Text.Wrap
                        }
                        NacreText {
                            id: trail
                            x: resultLabel.x
                            y: resultLabel.y + resultLabel.height + NacreTokens.space.xs
                            width: resultLabel.width
                            text: result.modelData.breadcrumb
                            color: NacreTokens.mutedInk
                            font.pointSize: NacreTokens.textSize.caption
                            wrapMode: Text.Wrap
                        }
                        NacreInteraction {
                            accessibleName: resultLabel.text + " " + trail.text
                            function onClicked() {
                                root.open(result.modelData.page + "#" + result.modelData.id);
                            }
                        }
                    }
                }
            }
            NacreText {
                width: parent.width
                visible: !root.query && !root.current?.prototype
                text: root.registry.text(root.page === "addons" ? "settings.registry.ui.install" : "settings.registry.ui.placeholder")
                wrapMode: Text.Wrap
                color: NacreTokens.mutedInk
            }
            Repeater {
                model: !root.query && root.current?.prototype ? root.sections : []
                delegate: SettingsCard {
                    id: card
                    required property var modelData
                    width: body.width
                    title: root.registry.text(modelData.labelId)
                    border.width: root.schema.settings.some(item => item.section === modelData.id && item.editorOwned && item.id === root.highlight) ? 1 : 0
                    border.color: NacreTokens.accent
                    description: root.registry.text(modelData.descriptionId)
                    Loader {
                        width: parent.width
                        active: card.modelData.customType === "panels-diagram"
                        sourceComponent: PanelsDiagram {
                            registry: root.registry
                            capabilities: root.capabilities
                            onNavigate: route => root.open(route)
                        }
                    }
                    Loader {
                        width: parent.width
                        active: card.modelData.customType === "colors-preview"
                        sourceComponent: ColorsPreview {
                            registry: root.registry
                            store: root.store
                        }
                    }
                    Loader {
                        objectName: root.schema.settings.find(item => item.section === card.modelData.id && item.editorOwned)?.id || "sharedEditor-" + card.modelData.id
                        width: parent.width
                        active: card.modelData.customType === "shared-editor"
                        enabled: !card.modelData.dependsOn || root.store.value(root.schema.settings.find(item => item.id === card.modelData.dependsOn)) === true
                        sourceComponent: SharedEditor {
                            editor: card.modelData.editor
                            screen: root.screen
                        }
                    }
                    Repeater {
                        model: Registry.settingsFor(root.schema, root.page, root.capabilities).filter(item => item.section === card.modelData.id && !item.editorOwned)
                        delegate: SettingsRow {
                            required property var modelData
                            width: card.width - 2 * NacreTokens.space.md
                            setting: modelData
                            registry: root.registry
                            store: root.store
                            highlighted: root.highlight === modelData.id
                        }
                    }
                    Repeater {
                        model: root.schema.actions.filter(item => item.section === card.modelData.id)
                        delegate: NacreSurface {
                            id: actionRow
                            required property var modelData
                            width: card.width - 2 * NacreTokens.space.md
                            implicitHeight: actionCopy.implicitHeight + 2 * NacreTokens.space.md
                            radius: NacreTokens.radii.small
                            color: "transparent"
                            Column {
                                id: actionCopy
                                x: NacreTokens.space.sm
                                y: NacreTokens.space.md
                                width: parent.width - NacreTokens.space.huge
                                spacing: NacreTokens.space.xs
                                NacreText {
                                    width: parent.width
                                    text: root.registry.text(actionRow.modelData.labelId)
                                    wrapMode: Text.Wrap
                                }
                                NacreText {
                                    width: parent.width
                                    text: root.registry.text(actionRow.modelData.descriptionId)
                                    color: NacreTokens.mutedInk
                                    font.pointSize: NacreTokens.textSize.caption
                                    wrapMode: Text.Wrap
                                }
                            }
                            NacreIcon {
                                anchors.right: parent.right
                                anchors.rightMargin: NacreTokens.space.sm
                                anchors.verticalCenter: parent.verticalCenter
                                text: actionRow.modelData.route ? "chevron_right" : "open_in_new"
                            }
                            NacreInteraction {
                                objectName: "action-" + actionRow.modelData.id
                                accessibleName: actionCopy.children[0].text
                                function onClicked() {
                                    root.action(actionRow.modelData);
                                }
                            }
                        }
                    }
                }
            }
            NacreText {
                width: parent.width
                visible: root.page === "colors" && !root.query
                text: root.registry.text("settings.registry.ui.credit")
                color: NacreTokens.mutedInk
                font.pointSize: NacreTokens.textSize.caption
            }
        }
    }
}
