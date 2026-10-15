pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import qs.widgets
import "../registry/registry.js" as Registry

FocusScope {
    id: root
    required property var registry
    required property var store
    property bool development: false
    property var entitySetting: null
    property var capabilities: ({})
    property var screen
    property string page: "panels"
    property string query: ""
    property string highlight: ""
    property string feedback: ""
    property var pendingAction: null
    property int resultIndex: 0
    property bool overflowOpen: false
    property var backStack: []
    readonly property var schema: registry.schema
    readonly property var current: schema.pages.find(item => item.id === page)
    readonly property bool narrow: width < NacreTokens.space.huge * 24
    readonly property var navigation: schema.pages.filter(item => !item.parent && Registry.visiblePage(schema, item, capabilities)).sort((a, b) => schema.groups.find(group => group.id === a.group).order - schema.groups.find(group => group.id === b.group).order || a.order - b.order)
    readonly property var sections: schema.sections.filter(item => item.page === page && Registry.allowed(item.visibleWhen, capabilities)).sort((a, b) => a.order - b.order)
    readonly property var results: Registry.search(schema, query, capabilities, registry.text, development)
    signal closeRequested
    signal routeOpened(string route)
    objectName: "settingsPrototype"
    focus: true
    function focusSearch() {
        search.forceActiveFocus();
    }
    function back() {
        if (!backStack.length)
            return open(current?.parent || "panels", false);
        const previous = backStack[backStack.length - 1];
        backStack = backStack.slice(0, -1);
        open(previous.route, false);
        Qt.callLater(() => scroll.contentY = Math.min(previous.position, Math.max(0, scroll.contentHeight - scroll.height)));
    }
    function open(link, remember = true) {
        const route = Registry.resolve(schema, link, capabilities, development);
        if (!route)
            return false;
        if (remember && current && route.page !== page)
            backStack = [...backStack,
                {
                    route: page + (highlight ? "#" + highlight : ""),
                    position: scroll.contentY
                }
            ];
        pendingAction = null;
        entitySetting = null;
        page = route.page;
        if (current?.enterAction)
            store.actionRequested(current.enterAction);
        highlight = route.setting;
        query = "";
        feedback = "";
        overflowOpen = false;
        pageMotion.cancel();
        scroll.contentY = 0;
        routeOpened(page + (highlight ? "#" + highlight : ""));
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
            open(result.route);
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
            else if (entitySetting)
                entitySetting = null;
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
        width: root.narrow ? root.width : NacreTokens.space.huge * 7 + NacreTokens.space.xl
        height: root.narrow ? NacreTokens.space.huge * 3 + NacreTokens.space.md : root.height
        radius: NacreTokens.radii.medium
        color: NacreTokens.raised
        NacreTextField {
            id: search
            objectName: "registrySearch"
            x: NacreTokens.space.md
            y: NacreTokens.space.md
            width: parent.width - 2 * x
            height: NacreTokens.space.huge + NacreTokens.space.md
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
            y: NacreTokens.space.huge * 2
            width: parent.width - 2 * x
            height: NacreTokens.space.huge + NacreTokens.space.xs
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
            currentIndex: root.navigation.findIndex(item => item.id === Registry.rootPage(root.schema, root.page))
            font.family: NacreTokens.textFamily
            Accessible.name: root.registry.text("settings.registry.ui.navigation")
            onActivated: index => root.open(root.navigation[index].id)
        }
        ListView {
            id: nav
            objectName: "registryNavigation"
            visible: !root.narrow
            x: NacreTokens.space.sm
            y: NacreTokens.space.huge * 2 + NacreTokens.space.sm
            width: parent.width - 2 * x
            height: parent.height - y - NacreTokens.space.sm
            clip: true
            model: root.navigation
            currentIndex: root.navigation.findIndex(item => item.id === Registry.rootPage(root.schema, root.page))
            boundsBehavior: Flickable.StopAtBounds
            spacing: NacreTokens.space.xs
            section.property: "group"
            section.delegate: NacreText {
                required property string section
                width: nav.width
                height: NacreTokens.space.huge + NacreTokens.space.hairline
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
                height: Math.max(NacreTokens.space.huge + NacreTokens.space.compact, label.implicitHeight + NacreTokens.space.md)
                radius: NacreTokens.radii.small
                color: nav.currentIndex === index ? Qt.alpha(NacreTokens.accent, .18) : "transparent"
                NacreIcon {
                    x: NacreTokens.space.md
                    anchors.verticalCenter: parent.verticalCenter
                    text: navItem.modelData.icon
                }
                NacreText {
                    id: label
                    x: NacreTokens.space.huge + NacreTokens.space.compact
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
            id: pageMotion
            view: scroll
        }
        Column {
            id: body
            width: scroll.width
            spacing: NacreTokens.space.xl
            Row {
                visible: (!!root.current?.parent || root.backStack.length > 0) && !root.query
                spacing: NacreTokens.space.sm
                ActionButton {
                    text: root.registry.text("settings.registry.ui.back")
                    icon: "arrow_back"
                    compact: true
                    onClicked: root.back()
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
                    text: root.registry.text(root.query ? "settings.registry.ui.results" : root.entitySetting?.labelId || root.current?.labelId)
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
                    root.pendingAction = {
                        adapter: "reset-page",
                        page: root.page,
                        confirmationId: "settings.registry.ui.reset-confirmation"
                    };
                    root.overflowOpen = false;
                }
            }
            NacreText {
                visible: root.feedback.length > 0 || root.store.issue(root.page).length > 0
                width: parent.width
                text: root.store.issue(root.page) || root.feedback
                wrapMode: Text.Wrap
                color: NacreTokens.mutedInk
                font.pointSize: NacreTokens.textSize.caption
            }
            SettingsCard {
                visible: !!root.pendingAction
                width: parent.width
                title: root.registry.text(root.pendingAction?.confirmationId || "settings.registry.ui.confirm-destructive")
                Row {
                    spacing: NacreTokens.space.sm
                    ActionButton {
                        objectName: "registryConfirmAction"
                        text: root.registry.text("settings.registry.ui.confirm")
                        onClicked: {
                            if (root.pendingAction.adapter === "reset-page")
                                root.store.reset(root.pendingAction.page);
                            else
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
                            text: result.modelData.breadcrumb + (result.modelData.kind === "setting" ? "\n" + root.registry.text("settings.registry.ui.current-value", {
                                    value: root.store.displayValue(root.schema.settings.find(item => item.id === result.modelData.id), root.registry.text)
                                }) : "")
                            color: NacreTokens.mutedInk
                            font.pointSize: NacreTokens.textSize.caption
                            wrapMode: Text.Wrap
                        }
                        NacreInteraction {
                            accessibleName: resultLabel.text + " " + trail.text
                            function onClicked() {
                                root.open(result.modelData.route);
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
            Loader {
                width: parent.width
                active: !!root.entitySetting && !root.query
                sourceComponent: EntityContent {
                    setting: root.entitySetting
                    registry: root.registry
                    store: root.store
                    onBack: root.entitySetting = null
                }
            }
            Repeater {
                model: !root.query && !root.entitySetting && root.current?.prototype ? root.sections.filter(section => section.customType || root.schema.settings.some(item => item.section === section.id && Registry.settingsFor(root.schema, root.page, root.capabilities, root.development).some(row => row.id === item.id)) || root.schema.actions.some(item => item.section === section.id && (root.development || item.backend !== "planned"))) : []
                delegate: SettingsCard {
                    id: card
                    required property var modelData
                    width: body.width
                    title: root.registry.text(modelData.labelId)
                    visible: modelData.customType !== "shared-editor" || !editorLoader.item || editorLoader.item.hasContent
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
                        width: parent.width
                        active: card.modelData.customType === "theme-bundles"
                        sourceComponent: ThemeBundles {}
                    }
                    Loader {
                        id: editorLoader
                        objectName: root.schema.settings.find(item => item.section === card.modelData.id && item.editorOwned)?.id || "sharedEditor-" + card.modelData.id
                        width: parent.width
                        active: card.modelData.customType === "shared-editor"
                        enabled: (!card.modelData.storeId || root.store.availableStores.includes(card.modelData.storeId)) && (!card.modelData.dependsOn || root.store.value(root.schema.settings.find(item => item.id === card.modelData.dependsOn)) === true)
                        opacity: enabled ? 1 : .45
                        sourceComponent: SharedEditor {
                            editor: card.modelData.editor
                            section: card.modelData.editorSection || "all"
                            screen: root.screen
                        }
                    }
                    Loader {
                        width: parent.width
                        active: card.modelData.customType === "device-content"
                        sourceComponent: DeviceContent {
                            target: card.modelData.target
                        }
                    }
                    Loader {
                        width: parent.width
                        active: card.modelData.customType === "wallpaper-picker"
                        sourceComponent: WallpaperContent {
                            registry: root.registry
                        }
                    }
                    Loader {
                        width: parent.width
                        active: card.modelData.customType === "publisher-targets"
                        sourceComponent: PublisherTargets {
                            registry: root.registry
                        }
                    }
                    Loader {
                        width: parent.width
                        active: card.modelData.customType === "shortcuts"
                        sourceComponent: ShortcutsContent {
                            registry: root.registry
                        }
                    }
                    Loader {
                        width: parent.width
                        active: card.modelData.customType === "addon-cards"
                        sourceComponent: AddonContent {
                            registry: root.registry
                            capabilities: root.capabilities
                            onAction: action => root.store.actionRequested(action)
                        }
                    }
                    Repeater {
                        model: Registry.settingsFor(root.schema, root.page, root.capabilities, root.development).filter(item => item.section === card.modelData.id && !item.editorOwned)
                        delegate: SettingsRow {
                            required property var modelData
                            width: card.width - 2 * NacreTokens.space.md
                            setting: modelData
                            registry: root.registry
                            store: root.store
                            highlighted: root.highlight === modelData.id
                            onEntitiesRequested: root.entitySetting = modelData
                        }
                    }
                    Repeater {
                        model: root.schema.actions.filter(item => item.section === card.modelData.id && Registry.allowed(item.visibleWhen, root.capabilities) && (root.development || item.backend !== "planned"))
                        delegate: NacreSurface {
                            id: actionRow
                            required property var modelData
                            objectName: modelData.id
                            border.width: root.highlight === modelData.id ? 1 : 0
                            border.color: NacreTokens.accent
                            width: card.width - 2 * NacreTokens.space.md
                            implicitHeight: actionCopy.implicitHeight + 2 * NacreTokens.space.md
                            radius: NacreTokens.radii.small
                            color: "transparent"
                            Column {
                                id: actionCopy
                                x: NacreTokens.space.sm
                                y: NacreTokens.space.md
                                width: Math.max(0, parent.width - (actionRow.modelData.destructive ? destructiveButton.width + NacreTokens.space.xxl : NacreTokens.space.huge))
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
                                visible: !actionRow.modelData.destructive
                                text: actionRow.modelData.route ? "chevron_right" : "open_in_new"
                            }
                            ActionButton {
                                id: destructiveButton
                                visible: actionRow.modelData.destructive === true
                                anchors.right: parent.right
                                anchors.verticalCenter: parent.verticalCenter
                                text: root.registry.text(actionRow.modelData.labelId)
                                compact: true
                                onClicked: root.action(actionRow.modelData)
                            }
                            NacreInteraction {
                                visible: !actionRow.modelData.destructive
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
                visible: root.page.split("/")[0] === "colors" && !root.query
                text: root.registry.text("settings.registry.ui.credit")
                color: NacreTokens.mutedInk
                font.pointSize: NacreTokens.textSize.caption
            }
        }
    }
}
