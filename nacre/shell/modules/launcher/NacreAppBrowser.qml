import QtQuick
import QtQuick.Controls.Basic
import Quickshell
import Quickshell.Io
import qs.widgets
import qs.services
import "menu-model.js" as Menu
import "calculator.js" as Calculator

NacreSurface {
    id: root
    required property var visibilities
    property var menu: NacreMenu
    property bool detached: false
    property bool closeAfterLaunch: false
    signal closeRequested
    property real viewportWidth: Quickshell.screens[0].width
    property real viewportHeight: Quickshell.screens[0].height
    property string route: "home"
    property var history: []
    property bool keyboardSelection: false
    property int selection: -1
    property var hoveredEntry: null
    property bool controlsPinned: false
    property var contextEntry: null
    property rect contextAnchor: Qt.rect(20, 84, 0, 0)
    property var confirmation: null
    property var form: null
    property var activePrompt: null
    readonly property string query: search?.text || ""
    readonly property var scope: Menu.scope(query)
    readonly property bool calculation: scope.kind === "calculator"
    readonly property var calculationResult: {
        if (!calculation)
            return {
                ok: false
            };
        const basic = Calculator.evaluate(scope.text);
        if (basic.ok || !/\b(to|in)\b/.test(scope.text))
            return basic;
        return root.menu.conversionQuery === scope.text ? root.menu.conversion : {
            ok: false,
            error: NacreI18n.tr("launcher.app-browser.error.converting", "Converting…")
        };
    }
    readonly property bool picker: !!activePrompt
    readonly property bool inputPicker: picker && root.activePrompt.kind === "input"
    readonly property bool appLayout: !picker && !scope.text && scope.kind === "all" && ["home", "apps", "hidden"].includes(route)
    readonly property bool showDetails: width >= 940 && !picker
    readonly property var entries: {
        if (picker && !inputPicker)
            return Menu.ranked((root.activePrompt.choices || []).map((entry, index) => Object.assign({}, entry, {
                    id: "choice-" + index,
                    kind: "choice",
                    name: entry.label,
                    operation: "choice",
                    icon: entry.icon || "chevron_right"
                })), query);
        if (calculation)
            return calculationResult.ok ? [
                {
                    id: "calculation",
                    kind: "calculation",
                    name: calculationResult.value,
                    label: calculationResult.value,
                    description: calculationResult.description || NacreI18n.tr("launcher.app-browser.description.enter-to-copy-the-result", "Enter to copy the result"),
                    icon: "calculate",
                    operation: "calculation",
                    value: calculationResult.value
                }
            ] : [];
        return root.menu.content(route, query);
    }
    readonly property bool hasSelection: keyboardSelection || !!scope.text || picker
    readonly property var selectedEntry: hasSelection && selection >= 0 ? entries[selection] || null : null
    readonly property var previewEntry: controlsPinned ? null : selectedEntry || hoveredEntry
    readonly property var breadcrumb: Menu.trail(route, root.menu.rows)
    readonly property real contentX: picker ? 20 : 182
    readonly property real contentWidth: width - contentX - (showDetails ? 270 : 20)
    readonly property real bodyHeight: height - 184
    implicitWidth: Math.min(1100, Math.max(540, viewportWidth - 80))
    implicitHeight: Math.min(590, Math.max(340, viewportHeight - 150))
    radius: 0
    color: "transparent"
    border.width: 0

    function focusExplicit() {
        search.forceActiveFocus();
    }
    function captureState() {
        return {
            route: route,
            query: query,
            history: history,
            selection: selection,
            keyboardSelection: keyboardSelection,
            scroll: {
                grid: grid.contentY,
                results: results.contentY,
                details: detailScroll.contentY
            }
        };
    }
    function restoreState(data) {
        route = data.route || "home";
        history = data.history || [];
        search.text = data.query || "";
        selection = data.selection ?? -1;
        keyboardSelection = data.keyboardSelection === true;
        Qt.callLater(() => {
            grid.contentY = data.scroll?.grid || 0;
            results.contentY = data.scroll?.results || 0;
            detailScroll.contentY = data.scroll?.details || 0;
        });
        root.menu.prepare(route, query, true);
    }
    function navigate(value, remember = true) {
        const resolved = value === "hidden" ? value : root.menu.resolve(value);
        if (!resolved) {
            root.menu.message = NacreI18n.tr("launcher.app-browser.message.that-section-is-not-available", "That section is not available");
            return;
        }
        const node = root.menu.node(resolved);
        if (remember && route !== resolved)
            history = [...history, route];
        route = node?.kind === "action" ? Menu.parentOf(resolved) || "home" : resolved;
        search.text = "";
        keyboardSelection = false;
        selection = -1;
        hoveredEntry = null;
        root.menu.prepare(route, "", true);
        if (node?.kind === "action") {
            selection = entries.findIndex(entry => entry.id === resolved);
            keyboardSelection = selection >= 0;
        }
        search.forceActiveFocus();
    }
    function select(name) {
        navigate(({
                favorites: "home",
                all: "apps",
                hidden: "hidden"
            })[name] || (root.menu.resolve(name) ? name : "apps"));
    }
    function resolveLeafRoute() {
        const node = root.menu.node(route);
        if (node?.kind !== "action")
            return;
        const selected = node.id;
        route = Menu.parentOf(selected) || "home";
        selection = entries.findIndex(entry => entry.id === selected);
        keyboardSelection = selection >= 0;
    }
    function reset() {
        activePrompt = NacreMenuPrompt.active && NacreMenuPrompt.ownerView === visibilities ? NacreMenuPrompt.request : null;
        history = [];
        route = visibilities.launcherRoute || "home";
        if (root.menu.rows.length && !root.menu.resolve(route) && route !== "hidden")
            route = "home";
        form = null;
        confirmation = null;
        context.close();
        hoveredEntry = null;
        controlsPinned = false;
        keyboardSelection = false;
        search.text = visibilities.launcherQuery || "";
        selection = picker ? Menu.firstEnabled(entries) : scope.text ? Menu.firstEnabled(entries) : -1;
        resolveLeafRoute();
        root.menu.prepare(route, query, true);
        if (inputPicker) {
            form = {
                kind: "input",
                title: root.activePrompt.title,
                fields: [
                    {
                        id: "value",
                        label: NacreI18n.tr("launcher.app-browser.label.value", "Value"),
                        initial: root.activePrompt.initial || ""
                    }
                ],
                button: "Continue"
            };
        }
        Qt.callLater(() => search.forceActiveFocus());
    }
    function back() {
        if (form) {
            cancelForm();
            return;
        }
        if (query) {
            search.text = "";
            return;
        }
        const previous = history.length ? history[history.length - 1] : Menu.parentOf(route) || "home";
        if (history.length)
            history = history.slice(0, -1);
        navigate(previous, false);
    }
    function favorite(entry) {
        if (entry && !["calculation", "choice"].includes(entry.kind) && !["open-file", "search-web"].includes(entry.operation)) {
            const ident = Menu.stable(entry);
            LauncherPreferences.update("favorite", ident, !LauncherPreferences.favorites.includes(ident));
        }
    }
    function run(entry, confirmed = false) {
        if (!entry)
            return;
        if (entry.kind === "choice") {
            NacreMenuPrompt.finish(entry.value, false);
            return;
        }
        if (entry.kind === "group") {
            navigate(entry.id);
            return;
        }
        if (entry.disabled) {
            actionsFor(entry);
            return;
        }
        if (entry.hidden && !query) {
            LauncherPreferences.update("hide", entry.id, false);
            return;
        }
        if (entry.operation === "scope") {
            search.text = entry.value;
            search.forceActiveFocus();
            return;
        }
        if (entry.operation === "calculator") {
            search.text = "= ";
            search.forceActiveFocus();
            return;
        }
        if (entry.operation === "route") {
            if (entry.value === "wallpaper") {
                NacrePanelState.openMode("wallpaper");
                return;
            }
            navigate(entry.value);
            return;
        }
        if (entry.operation === "form") {
            form = entry.value === "webapp" ? {
                kind: "webapp",
                title: NacreI18n.tr("launcher.app-browser.title.create-a-web-app", "Create a web app"),
                description: NacreI18n.tr("launcher.app-browser.description.add-a-website-to-all-apps", "Add a website to All apps."),
                fields: [
                    {
                        id: "label",
                        label: NacreI18n.tr("launcher.app-browser.label.app-name", "App name"),
                        placeholder: "My website"
                    },
                    {
                        id: "url",
                        label: NacreI18n.tr("launcher.app-browser.label.website", "Website"),
                        placeholder: "https://…"
                    }
                ],
                button: "Create app"
            } : {
                kind: "reminder",
                title: NacreI18n.tr("launcher.app-browser.title.set-a-reminder", "Set a reminder"),
                fields: [
                    {
                        id: "minutes",
                        label: NacreI18n.tr("launcher.app-browser.label.minutes", "Minutes"),
                        initial: "25"
                    },
                    {
                        id: "message",
                        label: NacreI18n.tr("launcher.app-browser.label.message", "Message"),
                        placeholder: "Time for a break"
                    }
                ],
                button: "Set reminder"
            };
            return;
        }
        if (entry.operation === "info") {
            hoveredEntry = entry;
            keyboardSelection = true;
            return;
        }
        if ((entry.confirm || entry.operation === "power" && entry.value !== "lock" || entry.operation === "maintenance" && entry.value === "restart") && !confirmed) {
            confirmation = entry;
            context.close();
            Qt.callLater(() => cancelConfirmation.forceActiveFocus());
            return;
        }
        if (entry.kind === "calculation") {
            AppLaunch.run(["wl-copy", "--", entry.value]);
            if (!detached || closeAfterLaunch) {
                visibilities.launcher = false;
                if (detached)
                    closeRequested();
            }
            return;
        }
        if (!root.menu.keepOpen(entry) && (!detached || closeAfterLaunch)) {
            visibilities.launcher = false;
            if (detached)
                closeRequested();
        }
        root.menu.execute(entry, confirmed);
    }
    function activate(index) {
        if (hasSelection)
            run(entries[index]);
    }
    function actionsFor(entry, anchor = null) {
        if (!entry || ["calculation", "choice"].includes(entry.kind))
            return;
        const target = anchor || (appLayout ? grid.currentItem : results.currentItem) || search;
        const point = target.mapToItem(root, 0, 0);
        contextAnchor = Qt.rect(point.x, point.y, target.width, target.height);
        contextEntry = entry;
        root.menu.inspectApp(entry);
        context.open();
    }
    function hoverEntry(entry, inside) {
        if (inside) {
            hoverClear.stop();
            controlsPinned = false;
            hoveredEntry = entry;
        } else
            hoverClear.restart();
    }
    function showQuickControls() {
        controlsPinned = true;
        hoveredEntry = null;
        keyboardSelection = false;
        selection = -1;
        detailScroll.contentY = 0;
        search.forceActiveFocus();
    }
    Timer {
        id: hoverClear
        interval: 160
        onTriggered: if (!detailHover.hovered)
            root.hoveredEntry = null
    }
    function focusResults(delta = 1) {
        controlsPinned = false;
        keyboardSelection = true;
        selection = Menu.step(entries, selection, delta);
        if (appLayout) {
            grid.forceActiveFocus();
            grid.positionViewAtIndex(Math.max(0, selection), GridView.Contain);
        } else {
            results.forceActiveFocus();
            results.positionViewAtIndex(Math.max(0, selection), ListView.Contain);
        }
    }
    function keys(event) {
        if (event.key === Qt.Key_D && event.modifiers & Qt.ControlModifier) {
            favorite(selectedEntry);
            event.accepted = true;
            return;
        }
        if (event.key === Qt.Key_Tab) {
            if (selectedEntry)
                actionsFor(selectedEntry);
            else
                rail.forceActiveFocus();
            event.accepted = true;
            return;
        }
        if (event.key === Qt.Key_Escape) {
            dismiss();
            event.accepted = true;
            return;
        }
        if (event.key === Qt.Key_Left && !appLayout && !query) {
            back();
            event.accepted = true;
            return;
        }
        if (event.key === Qt.Key_Right && !appLayout && selectedEntry) {
            selectedEntry.kind === "group" ? navigate(selectedEntry.id) : actionsFor(selectedEntry);
            event.accepted = true;
            return;
        }
        if (event.key === Qt.Key_Up || event.key === Qt.Key_Down || appLayout && [Qt.Key_Left, Qt.Key_Right].includes(event.key)) {
            const columns = Math.max(1, Math.floor(grid.width / grid.cellWidth));
            const direction = [Qt.Key_Up, Qt.Key_Left].includes(event.key) ? -1 : 1;
            keyboardSelection = true;
            if (selection < 0)
                selection = direction > 0 ? Menu.firstEnabled(entries) : Menu.step(entries, -1, -1);
            else if (appLayout && [Qt.Key_Up, Qt.Key_Down].includes(event.key))
                selection = Math.max(0, Math.min(entries.length - 1, selection + direction * columns));
            else
                selection = Menu.step(entries, selection, direction);
            if (appLayout)
                grid.positionViewAtIndex(Math.max(0, selection), GridView.Contain);
            else
                results.positionViewAtIndex(Math.max(0, selection), ListView.Contain);
            event.accepted = true;
            return;
        }
        if (event.text && !(event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier))) {
            search.text += event.text;
            search.forceActiveFocus();
            event.accepted = true;
        } else if (event.key === Qt.Key_Backspace) {
            query ? search.text = search.text.slice(0, -1) : back();
            search.forceActiveFocus();
            event.accepted = true;
        }
    }
    function dismiss() {
        if (context.opened) {
            context.close();
            search.forceActiveFocus();
        } else if (confirmation)
            confirmation = null;
        else if (form)
            cancelForm();
        else if (picker)
            NacreMenuPrompt.finish("", true);
        else if (detached) {
            if (query)
                search.text = "";
            else if (route !== "home")
                navigate("home", false);
        } else
            visibilities.launcher = false;
    }
    function cancelForm() {
        if (inputPicker)
            NacreMenuPrompt.finish("", true);
        form = null;
        search.forceActiveFocus();
    }
    function submitForm(values) {
        if (inputPicker)
            NacreMenuPrompt.finish(values.value || "", false);
        else
            root.menu.work(form.kind === "webapp" ? "webapp-create" : "reminder", "", values);
        form = null;
        search.forceActiveFocus();
    }
    onQueryChanged: {
        controlsPinned = false;
        keyboardSelection = false;
        hoveredEntry = null;
        selection = scope.text || picker ? Menu.firstEnabled(entries) : -1;
        queryDelay.restart();
    }
    onEntriesChanged: {
        if (scope.text || picker)
            selection = Menu.firstEnabled(entries);
        else if (!keyboardSelection)
            selection = -1;
        else if (selection >= entries.length)
            selection = entries.length - 1;
    }
    Component.onCompleted: reset()
    Connections {
        target: root.visibilities
        function onLauncherChanged() {
            if (root.visibilities.launcher)
                root.reset();
        }
        function onLauncherRequestChanged() {
            if (root.visibilities.launcher)
                Qt.callLater(root.reset);
        }
    }
    Connections {
        target: root.menu
        function onCatalogChanged() {
            if (root.visibilities.launcher && !root.query)
                root.resolveLeafRoute();
        }
    }
    Connections {
        target: NacreMenuPrompt
        function onRequestChanged() {
            if (NacreMenuPrompt.request && NacreMenuPrompt.ownerView === root.visibilities)
                root.reset();
        }
    }
    Timer {
        id: queryDelay
        interval: 180
        onTriggered: root.menu.prepare(root.route, root.query, false)
    }

    Item {
        id: navigation
        visible: !root.picker
        x: 12
        y: 16
        width: 152
        height: root.height - 98
        Row {
            spacing: 9
            BrandLogo {
                width: 28
                height: 28
            }
            NacreText {
                anchors.verticalCenter: parent.verticalCenter
                text: NacreI18n.tr("launcher.app-browser.text.nacre", "Nacre")
                font.pointSize: 15
            }
        }
        ListView {
            id: rail
            objectName: "launcherCategories"
            y: 44
            width: parent.width
            height: parent.height - y - 38
            clip: true
            model: root.menu.navigation
            spacing: 3
            currentIndex: Math.max(0, root.menu.navigation.findIndex(entry => root.route.split(".")[0] === entry.id))
            delegate: NacreSurface {
                required property var modelData
                required property int index
                width: rail.width
                height: Math.min(37, Math.max(28, (rail.height - (root.menu.navigation.length - 1) * 3) / Math.max(1, root.menu.navigation.length)))
                radius: 9
                color: root.route.split(".")[0] === modelData.id ? Qt.tint(NacreColours.palette.m3surface, Qt.alpha(NacreColours.palette.m3onSurface, .12)) : "transparent"
                border.width: root.route.split(".")[0] === modelData.id ? 1 : 0
                border.color: Qt.alpha(NacreColours.palette.m3onSurface, .16)
                Rectangle {
                    x: 0
                    y: 8
                    width: 3
                    height: parent.height - 16
                    radius: 1.5
                    color: NacreColours.palette.m3primary
                    visible: root.route.split(".")[0] === parent.modelData.id
                    gradient: Gradient {
                        GradientStop {
                            position: 0
                            color: NacreColours.palette.m3primary
                        }
                        GradientStop {
                            position: .5
                            color: NacreColours.palette.m3secondary
                        }
                        GradientStop {
                            position: 1
                            color: NacreColours.palette.m3tertiary
                        }
                    }
                }
                Row {
                    x: 11
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 10
                    NacreIcon {
                        text: parent.parent.modelData.icon
                        font.pointSize: 15
                    }
                    NacreText {
                        text: parent.parent.modelData.label
                        font.pointSize: 10.5
                        textFormat: Text.PlainText
                    }
                }
                NacreInteraction {
                    function onClicked() {
                        root.navigate(parent.modelData.id);
                    }
                }
            }
            Keys.onReturnPressed: if (currentItem)
                root.navigate(currentItem.modelData.id)
            Keys.onRightPressed: root.focusResults()
            Keys.onEscapePressed: root.dismiss()
            Keys.onPressed: event => {
                if (event.text)
                    root.keys(event);
            }
            FastScroll {
                view: rail
                step: 160
            }
        }
        NacreMenuButton {
            anchors.bottom: parent.bottom
            text: NacreI18n.tr("launcher.app-browser.text.hidden-apps", "Hidden apps")
            compact: true
            visible: LauncherPreferences.hidden.length > 0
            onClicked: root.navigate("hidden")
        }
    }
    Rectangle {
        x: 172
        y: 18
        width: 1
        height: parent.height - 110
        color: NacreColours.palette.m3outlineVariant
        visible: !root.picker
    }
    Row {
        id: path
        x: root.contentX
        y: 17
        height: 32
        spacing: 8
        visible: !root.picker
        NacreMenuButton {
            objectName: "launcherBack"
            anchors.verticalCenter: parent.verticalCenter
            compact: true
            icon: "chevron_left"
            visible: root.route !== "home" || !!root.query
            onClicked: root.back()
        }
        Repeater {
            model: root.breadcrumb
            delegate: Row {
                required property var modelData
                required property int index
                objectName: "launcherHeading"
                anchors.verticalCenter: parent.verticalCenter
                spacing: 5
                NacreText {
                    text: "›"
                    visible: parent.index > 0
                    color: NacreColours.palette.m3onSurfaceVariant
                }
                NacreText {
                    text: parent.modelData.label
                    font.pointSize: 12
                    textFormat: Text.PlainText
                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: root.navigate(parent.parent.modelData.id)
                    }
                }
            }
        }
    }
    NacreText {
        x: root.contentX
        y: root.picker ? 16 : 55
        text: root.picker ? root.activePrompt.title : root.calculation ? NacreI18n.tr("launcher.app-browser.title.calculator", "Calculator") : root.scope.kind === "brain" ? NacreI18n.tr("launcher.app-browser.title.nacre-brain", "Nacre Brain") : root.scope.kind === "ai" ? NacreI18n.tr("launcher.app-browser.title.nacre-ai", "Nacre AI") : root.scope.kind === "help" ? NacreI18n.tr("launcher.app-browser.title.search-scopes", "Search scopes") : root.scope.text ? NacreI18n.tr("launcher.app-browser.title.search-everything", "Search everything") : root.route === "home" ? NacreI18n.tr("launcher.app-browser.title.favorites", "Favorites") : root.route === "hidden" ? NacreI18n.tr("launcher.app-browser.title.hidden-applications", "Hidden applications") : root.menu.node(root.route)?.description || ""
        font.pointSize: root.picker ? 15 : 10
        color: NacreColours.palette.m3onSurfaceVariant
        textFormat: Text.PlainText
        width: root.contentWidth
        elide: Text.ElideRight
    }
    GridView {
        id: grid
        objectName: "launcherApps"
        visible: root.appLayout && !root.form
        x: root.contentX
        y: 84
        width: root.contentWidth
        height: root.bodyHeight
        cellWidth: width / Math.max(1, Math.floor(width / 116))
        cellHeight: Math.max(72, Math.min(100, height / 5))
        model: root.entries
        currentIndex: root.selection
        keyNavigationEnabled: false
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        delegate: NacreSurface {
            id: tile
            required property var modelData
            required property int index
            readonly property bool selected: root.hasSelection && root.selection === index
            readonly property bool compactTile: grid.cellHeight < 72
            objectName: "launcherTile_" + index
            width: grid.cellWidth - 6
            height: grid.cellHeight - 5
            radius: 11
            color: selected || tileHover.hovered ? Qt.tint(NacreColours.palette.m3surface, Qt.alpha(NacreColours.palette.m3onSurface, selected ? .09 : .04)) : "transparent"
            border.width: selected ? 1 : 0
            border.color: NacreColours.palette.m3outlineVariant
            Rectangle {
                x: 0
                y: 10
                width: 3
                height: parent.height - 20
                radius: 1.5
                color: NacreColours.palette.m3primary
                visible: tile.selected
            }
            HoverHandler {
                id: tileHover
                onHoveredChanged: root.hoverEntry(tile.modelData, hovered)
            }
            NacreMenuIcon {
                entry: tile.modelData
                size: tile.compactTile ? 28 : Math.min(44, Math.max(28, grid.cellHeight - 32))
                width: size
                height: size
                x: tile.compactTile ? 8 : (parent.width - width) / 2
                y: tile.compactTile ? (parent.height - height) / 2 : 7
            }
            NacreText {
                x: tile.compactTile ? 42 : 7
                y: tile.compactTile ? (parent.height - 20) / 2 : Math.min(55, grid.cellHeight - 23)
                width: parent.width - x - (tile.compactTile && tileHover.hovered ? 30 : 7)
                height: tile.compactTile ? 20 : parent.height - y - 4
                text: tile.modelData.name
                textFormat: Text.PlainText
                font.pointSize: 10
                wrapMode: Text.Wrap
                maximumLineCount: tile.compactTile ? 1 : 2
                elide: Text.ElideRight
                horizontalAlignment: tile.compactTile ? Text.AlignLeft : Text.AlignHCenter
            }
            MouseArea {
                anchors.fill: parent
                acceptedButtons: Qt.LeftButton | Qt.RightButton
                onClicked: event => {
                    root.keyboardSelection = true;
                    root.selection = tile.index;
                    event.button === Qt.RightButton ? root.actionsFor(tile.modelData, tile) : root.run(tile.modelData);
                }
            }
            NacreSurface {
                objectName: "launcherFavorite_" + tile.index
                x: parent.width - 24
                y: tile.compactTile ? (parent.height - height) / 2 : 3
                width: 23
                height: 23
                radius: 7
                color: "transparent"
                visible: tileHover.hovered || (root.route !== "home" && tile.selected)
                NacreIcon {
                    anchors.centerIn: parent
                    text: "favorite"
                    fill: LauncherPreferences.favorites.includes(Menu.stable(tile.modelData)) ? 1 : 0
                    font.pointSize: 11
                    color: NacreColours.palette.m3primary
                }
                NacreInteraction {
                    function onClicked() {
                        root.favorite(tile.modelData);
                    }
                }
            }
        }
        Keys.onReturnPressed: root.activate(root.selection)
        Keys.onPressed: event => root.keys(event)
        FastScroll {
            view: grid
            step: 300
        }
    }
    ListView {
        id: results
        objectName: "launcherResults"
        visible: !root.appLayout && !root.form
        x: root.contentX
        y: root.picker ? 54 : 84
        width: root.contentWidth
        height: Math.floor(root.bodyHeight / 58) * 58 + (root.picker ? 30 : 0)
        model: root.entries
        currentIndex: root.selection
        keyNavigationEnabled: false
        spacing: 3
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        delegate: NacreSurface {
            id: row
            required property var modelData
            required property int index
            readonly property bool selected: root.hasSelection && root.selection === index
            objectName: "launcherRow_" + index
            width: results.width
            height: 55
            radius: 10
            color: selected || rowHover.hovered ? Qt.tint(NacreColours.palette.m3surface, Qt.alpha(NacreColours.palette.m3onSurface, selected ? .085 : .04)) : "transparent"
            opacity: modelData.disabled ? .58 : 1
            Rectangle {
                x: 0
                y: 9
                width: 3
                height: parent.height - 18
                radius: 1.5
                color: NacreColours.palette.m3primary
                visible: row.selected
            }
            HoverHandler {
                id: rowHover
                onHoveredChanged: root.hoverEntry(row.modelData, hovered)
            }
            NacreMenuIcon {
                entry: row.modelData
                size: 30
                width: 30
                height: 30
                x: 12
                anchors.verticalCenter: parent.verticalCenter
            }
            NacreText {
                x: 53
                y: 8
                width: parent.width - x - 100
                text: row.modelData.name
                font.pointSize: 11
                textFormat: Text.PlainText
                elide: Text.ElideRight
            }
            NacreText {
                x: 53
                y: 31
                width: parent.width - x - 32
                text: row.modelData.breadcrumb || row.modelData.description || ""
                font.pointSize: 9
                textFormat: Text.PlainText
                elide: Text.ElideRight
                color: NacreColours.palette.m3onSurfaceVariant
            }
            NacreText {
                anchors.right: parent.right
                anchors.rightMargin: 11
                y: 10
                text: row.modelData.shortcut || (row.modelData.kind === "group" ? "›" : row.modelData.checked ? "✓ " + (row.modelData.status || "") : row.modelData.status || "")
                font.pointSize: 9
                color: NacreColours.palette.m3onSurfaceVariant
                textFormat: Text.PlainText
            }
            MouseArea {
                anchors.fill: parent
                acceptedButtons: Qt.LeftButton | Qt.RightButton
                onClicked: event => {
                    root.keyboardSelection = true;
                    root.selection = row.index;
                    event.button === Qt.RightButton ? root.actionsFor(row.modelData, row) : root.run(row.modelData);
                }
            }
        }
        Keys.onReturnPressed: root.activate(root.selection)
        Keys.onPressed: event => root.keys(event)
        FastScroll {
            view: results
            step: 300
        }
    }
    NacreText {
        x: root.contentX + 14
        y: root.picker ? 80 : 120
        width: root.contentWidth - 28
        visible: !root.entries.length && !root.form
        text: root.calculation ? root.calculationResult.error || NacreI18n.tr("launcher.app-browser.text.type-an-expression-after", "Type an expression after =") : root.scope.kind === "files" ? root.menu.fileMessage || NacreI18n.tr("launcher.app-browser.text.type-and-a-filename-to-search-your-folders", "Type : and a filename to search your folders") : root.scope.kind === "brain" ? (root.scope.text ? NacreI18n.tr("launcher.app-browser.text.looking-in-your-private-notes", "Looking in your private notes…") : NacreI18n.tr("launcher.app-browser.text.type-and-a-few-words-to-search-nacre-brain", "Type # and a few words to search Nacre Brain")) : root.scope.kind === "ai" ? NacreI18n.tr("launcher.app-browser.text.type-and-your-question", "Type ? and your question") : root.route === "home" && !root.query ? NacreI18n.tr("launcher.app-browser.text.add-apps-and-menu-items-to-favorites-with-ctrl-d-or-the", "Add apps and menu items to Favorites with Ctrl+D or their item actions.") : root.route === "software.packages" ? root.menu.packageMessage || NacreI18n.tr("launcher.app-browser.text.type-two-or-more-characters-to-search-packages", "Type two or more characters to search packages") : NacreI18n.tr("launcher.app-browser.text.no-matches-try-a-different-word-for-actions-or-for-sear", "No matches. Try a different word, > for actions, or / for search scopes.")
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
        color: NacreColours.palette.m3onSurfaceVariant
        font.pointSize: 11
    }
    NacreSurface {
        id: details
        visible: root.showDetails && !root.form
        x: parent.width - 252
        y: 84
        width: 234
        height: root.bodyHeight
        radius: 13
        color: Qt.tint(NacreColours.palette.m3surface, Qt.alpha(NacreColours.palette.m3onSurface, .025))
        HoverHandler {
            id: detailHover
            onHoveredChanged: hovered ? hoverClear.stop() : hoverClear.restart()
        }
        Flickable {
            id: detailScroll
            anchors.fill: parent
            anchors.margins: 13
            clip: true
            contentHeight: detailColumn.implicitHeight
            boundsBehavior: Flickable.StopAtBounds
            Column {
                id: detailColumn
                width: parent.width
                spacing: 8
                NacreMenuButton {
                    objectName: "launcherBackToControls"
                    visible: !!root.previewEntry
                    compact: true
                    icon: "chevron_left"
                    text: NacreI18n.tr("launcher.app-browser.text.quick-controls", "Quick controls")
                    onClicked: root.showQuickControls()
                }
                NacreMenuIcon {
                    visible: !!root.previewEntry
                    entry: root.previewEntry || {}
                    size: 48
                    width: 48
                    height: 48
                }
                NacreText {
                    width: parent.width
                    visible: !!root.previewEntry
                    text: root.previewEntry?.name || ""
                    textFormat: Text.PlainText
                    font.pointSize: 14
                    wrapMode: Text.Wrap
                    maximumLineCount: 3
                    elide: Text.ElideRight
                }
                Image {
                    width: parent.width
                    height: width * 9 / 16
                    visible: !!root.previewEntry?.preview && root.previewEntry?.operation === "wallpaper"
                    source: visible ? root.previewEntry.preview : ""
                    sourceSize.width: 512
                    sourceSize.height: 288
                    asynchronous: true
                    fillMode: Image.PreserveAspectCrop
                }
                NacreText {
                    width: parent.width
                    visible: !!root.previewEntry
                    text: root.previewEntry?.reason || root.previewEntry?.description || ""
                    textFormat: Text.PlainText
                    font.pointSize: 10.5
                    color: NacreColours.palette.m3onSurfaceVariant
                    wrapMode: Text.Wrap
                    maximumLineCount: 7
                    elide: Text.ElideRight
                }
                NacreText {
                    width: parent.width
                    visible: !!root.previewEntry?.previewFont
                    text: NacreI18n.tr("launcher.app-browser.text.nacre-colors-shaped-by-your-wallpaper", "Nacre\nColors shaped by your wallpaper.")
                    textFormat: Text.PlainText
                    font.family: root.previewEntry?.previewFont || NacreTokens.textFamily
                    font.pointSize: 16
                    wrapMode: Text.Wrap
                }
                NacreText {
                    width: parent.width
                    visible: root.previewEntry?.operation === "info"
                    text: root.previewEntry?.value === "health" ? NacreI18n.tr("ui.app-browser.text.services-value0", "Services\n{value0}", {
                        value0: Object.keys(Maintenance.data.services || {}).map(key => key + ": " + Maintenance.data.services[key]).join("\n")
                    }) : root.previewEntry?.value === "guide" ? NacreI18n.tr("launcher.app-browser.text.super-a-opens-this-menu-search-finds-apps-and-controls", "Super+Space opens this menu. Search finds apps and controls at any depth.\n\nUse > for commands, = for arithmetic, # for private notes and ? for an explicit AI question.\n\nEnter opens the selected item. Right enters a section; Tab opens item actions. Backspace with an empty query goes back. Escape closes.\n\nCtrl+D adds or removes a favorite.") : root.previewEntry?.description || ""
                    textFormat: Text.PlainText
                    font.pointSize: 10
                    wrapMode: Text.Wrap
                }
                NacreMenuButton {
                    width: parent.width
                    visible: !!root.previewEntry && root.previewEntry.kind !== "group" && !root.previewEntry.disabled && root.previewEntry.operation !== "info"
                    text: root.previewEntry?.operation === "wallpaper" ? NacreI18n.tr("launcher.app-browser.text.use-wallpaper", "Use wallpaper") : root.previewEntry?.operation === "font" ? NacreI18n.tr("launcher.app-browser.text.use-font", "Use font") : root.previewEntry?.operation === "install-package" ? NacreI18n.tr("launcher.app-browser.text.install", "Install") : root.previewEntry?.kind === "app" ? NacreI18n.tr("launcher.app-browser.text.open-app", "Open app") : NacreI18n.tr("launcher.app-browser.text.run", "Run")
                    onClicked: root.run(root.previewEntry)
                }
                NacreMenuButton {
                    id: detailActions
                    objectName: "launcherDetailActions"
                    width: parent.width
                    visible: !!root.previewEntry
                    text: root.previewEntry?.kind === "group" ? NacreI18n.tr("launcher.app-browser.text.open-section", "Open section") : NacreI18n.tr("launcher.app-browser.text.item-actions", "Item actions")
                    onClicked: root.previewEntry.kind === "group" ? root.navigate(root.previewEntry.id) : root.actionsFor(root.previewEntry, detailActions)
                }
                NacreText {
                    text: NacreI18n.tr("launcher.app-browser.text.recent", "Recent")
                    font.pointSize: 10
                    visible: !root.previewEntry && root.menu.recent().length > 0
                }
                Repeater {
                    model: root.previewEntry ? [] : root.menu.recent().slice(0, 3)
                    delegate: NacreMenuButton {
                        required property var modelData
                        width: detailColumn.width
                        compact: true
                        text: modelData.name
                        onClicked: root.run(modelData)
                    }
                }
                NacreText {
                    text: NacreI18n.tr("launcher.app-browser.text.quick-controls", "Quick controls")
                    font.pointSize: 10
                    visible: !root.previewEntry
                }
                Repeater {
                    model: root.previewEntry ? [] : ["actions.controls.dnd", "actions.controls.night-light", "actions.controls.stay-awake"]
                    delegate: NacreMenuButton {
                        required property string modelData
                        readonly property var item: root.menu.node(modelData)
                        objectName: "launcherToggle_" + modelData.split(".").pop()
                        toggle: true
                        description: item?.reason || ({
                                "actions.controls.dnd": NacreI18n.tr("launcher.app-browser.description.silence-notification-popups", "Silence notification popups"),
                                "actions.controls.night-light": NacreI18n.tr("launcher.app-browser.description.make-the-screen-warmer", "Make the screen warmer"),
                                "actions.controls.stay-awake": NacreI18n.tr("launcher.app-browser.description.prevent-automatic-sleep", "Prevent automatic sleep")
                            })[modelData] || ""
                        stateText: item?.disabled ? "—" : ""
                        width: detailColumn.width
                        compact: true
                        selected: !!item && root.menu.stateFor(item)
                        enabled: !!item && !item.disabled && (modelData !== "actions.controls.night-light" || !NacreControlTools.busy)
                        icon: selected ? "check" : item?.icon || ""
                        text: item?.label || ""
                        onClicked: root.menu.execute(item)
                    }
                }
                NacreMenuButton {
                    width: parent.width
                    visible: !root.previewEntry
                    compact: true
                    text: Updates.count ? NacreI18n.tr("ui.app-browser.text.value0-updates", "{value0} updates", {
                        value0: Updates.count
                    }) : NacreI18n.tr("launcher.app-browser.text.up-to-date", "Up to date")
                    icon: "system_update"
                    onClicked: root.run(root.menu.node("health.updates"))
                }
            }
            FastScroll {
                view: detailScroll
                step: 220
            }
        }
    }
    NacreText {
        x: 20
        y: parent.height - 88
        width: parent.width - 40
        text: root.menu.catalog.error || root.menu.message || (root.calculation ? NacreI18n.tr("launcher.app-browser.text.enter-copies-the-result", "Enter copies the result") : root.picker ? NacreI18n.tr("launcher.app-browser.text.enter-chooses-escape-cancels", "Enter chooses · Escape cancels") : NacreI18n.tr("launcher.app-browser.text.enter-opens-tab-item-actions-ctrl-d-favorite", "Enter opens · Tab item actions · Ctrl+D favorite"))
        textFormat: Text.PlainText
        font.pointSize: 9
        color: NacreColours.palette.m3onSurfaceVariant
        elide: Text.ElideRight
        visible: !root.form && !root.confirmation && (!!root.selectedEntry || root.picker || !!root.menu.catalog.error || !!root.menu.message)
    }
    NacreTextField {
        id: search
        objectName: "launcherSearch"
        x: 16
        y: parent.height - 67
        width: parent.width - 32
        height: 49
        padding: 15
        visible: !root.form && !root.confirmation
        placeholderText: root.picker ? NacreI18n.tr("launcher.app-browser.placeholderText.filter-choices", "Filter choices") : root.calculation ? "= (12 + 8) / 5" : NacreI18n.tr("launcher.app-browser.placeholderText.search-everything", "Search everything…")
        rightPadding: prefixHints.visible ? prefixHints.width + 28 : 15
        onAccepted: root.activate(root.selection)
        Keys.onDownPressed: root.focusResults()
        Keys.onEscapePressed: root.dismiss()
        Keys.onPressed: event => {
            if (event.key === Qt.Key_D && event.modifiers & Qt.ControlModifier) {
                root.favorite(root.selectedEntry);
                event.accepted = true;
            } else if (event.key === Qt.Key_Tab) {
                root.selectedEntry ? root.actionsFor(root.selectedEntry) : rail.forceActiveFocus();
                event.accepted = true;
            } else if (event.key === Qt.Key_Backspace && !text) {
                root.back();
                event.accepted = true;
            }
        }
    }
    Row {
        id: prefixHints
        anchors.right: search.right
        anchors.rightMargin: 14
        anchors.verticalCenter: search.verticalCenter
        spacing: 6
        visible: search.visible && !root.picker && !root.query && root.width >= 940
        Repeater {
            model: ["> Actions", "= Calculate", "# Brain", ": Files", "? AI"]
            delegate: NacreSurface {
                required property string modelData
                width: hint.implicitWidth + 14
                height: 24
                radius: 6
                color: Qt.alpha(NacreColours.palette.m3onSurface, .045)
                NacreText {
                    id: hint
                    anchors.centerIn: parent
                    text: NacreI18n.option("prefix-hint", parent.modelData)
                    font.pointSize: 8.5
                    color: NacreColours.palette.m3onSurfaceVariant
                }
            }
        }
    }
    Loader {
        anchors.fill: parent
        anchors.leftMargin: root.picker ? 16 : 182
        anchors.rightMargin: 16
        anchors.topMargin: 65
        anchors.bottomMargin: 30
        active: !!root.form
        sourceComponent: NacreMenuForm {
            form: root.form || {}
            onSubmitted: values => root.submitForm(values)
            onCancelled: root.cancelForm()
        }
    }
    NacreSurface {
        anchors.fill: parent
        visible: !!root.confirmation
        color: NacreColours.palette.m3surface
        radius: 0
        MouseArea {
            anchors.fill: parent
        }
        Column {
            anchors.centerIn: parent
            width: Math.min(parent.width - 60, 500)
            spacing: 18
            NacreIcon {
                text: root.confirmation?.icon || "help"
                font.pointSize: 32
                color: NacreColours.palette.m3error
            }
            NacreText {
                width: parent.width
                text: root.confirmation?.confirm || root.confirmation?.name + "?"
                textFormat: Text.PlainText
                font.pointSize: 20
                wrapMode: Text.Wrap
            }
            NacreText {
                width: parent.width
                text: root.confirmation?.operation === "power" ? (root.confirmation?.value === "suspend" ? NacreI18n.tr("launcher.app-browser.text.your-applications-stay-open-while-the-laptop-sleeps", "Your applications stay open while the laptop sleeps.") : NacreI18n.tr("launcher.app-browser.text.this-will-close-your-running-apps-and-tasks-save-your-w", "This will close your running apps and tasks. Save your work before continuing.")) : NacreI18n.tr("launcher.app-browser.text.the-desktop-shell-will-reload-your-applications-stay-ru", "The desktop shell will reload. Your applications stay running.")
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                color: NacreColours.palette.m3onSurfaceVariant
            }
            Row {
                spacing: 12
                NacreMenuButton {
                    id: cancelConfirmation
                    text: NacreI18n.tr("launcher.app-browser.text.cancel", "Cancel")
                    onClicked: {
                        root.confirmation = null;
                        search.forceActiveFocus();
                    }
                }
                NacreMenuButton {
                    text: root.confirmation?.name || NacreI18n.tr("launcher.app-browser.text.continue", "Continue")
                    onClicked: {
                        const entry = root.confirmation;
                        root.confirmation = null;
                        root.run(entry, true);
                    }
                }
            }
        }
        Keys.onEscapePressed: root.confirmation = null
    }
    Popup {
        id: context
        objectName: "launcherItemActions"
        parent: root
        width: Math.min(240, root.width - 16)
        x: Math.max(8, Math.min(root.width - width - 8, root.contextAnchor.x + root.contextAnchor.width + width + 14 <= root.width ? root.contextAnchor.x + root.contextAnchor.width + 6 : root.contextAnchor.x - width - 6))
        y: Math.max(8, Math.min(root.height - height - 86, root.contextAnchor.y))
        padding: 12
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        background: NacreSurface {
            color: NacreColours.palette.m3surfaceContainerHigh
            radius: 13
            border.width: 1
            border.color: NacreColours.palette.m3outlineVariant
        }
        contentItem: Flickable {
            id: contextScroll
            implicitHeight: Math.min(actionColumn.implicitHeight, Math.max(80, root.height - 118))
            contentWidth: width
            contentHeight: actionColumn.implicitHeight
            boundsBehavior: Flickable.StopAtBounds
            clip: true
            Column {
                id: actionColumn
                width: parent.width
                spacing: 7
                NacreText {
                    width: parent.width
                    text: root.contextEntry?.name || NacreI18n.tr("launcher.app-browser.text.item-actions", "Item actions")
                    font.pointSize: 11
                    wrapMode: Text.Wrap
                    maximumLineCount: 2
                    elide: Text.ElideRight
                    textFormat: Text.PlainText
                }
                NacreMenuButton {
                    width: parent.width
                    compact: true
                    text: NacreI18n.tr("launcher.app-browser.text.open", "Open")
                    visible: !!root.contextEntry && !root.contextEntry.disabled
                    onClicked: {
                        const entry = root.contextEntry;
                        context.close();
                        root.run(entry);
                    }
                }
                NacreMenuButton {
                    width: parent.width
                    compact: true
                    visible: !!root.contextEntry && !["open-file", "search-web"].includes(root.contextEntry.operation)
                    text: root.contextEntry && LauncherPreferences.favorites.includes(Menu.stable(root.contextEntry)) ? NacreI18n.tr("launcher.app-browser.text.remove-favorite", "Remove favorite") : NacreI18n.tr("launcher.app-browser.text.add-favorite", "Add favorite")
                    onClicked: {
                        root.favorite(root.contextEntry);
                        context.close();
                    }
                }
                NacreMenuButton {
                    width: parent.width
                    compact: true
                    visible: root.contextEntry?.kind === "app"
                    text: root.contextEntry?.hidden ? NacreI18n.tr("launcher.app-browser.text.show-application", "Show application") : NacreI18n.tr("launcher.app-browser.text.hide-application", "Hide application")
                    onClicked: {
                        LauncherPreferences.update("hide", root.contextEntry.id, !root.contextEntry.hidden);
                        context.close();
                    }
                }
                NacreMenuButton {
                    width: parent.width
                    compact: true
                    visible: (!!root.contextEntry?.package && root.contextEntry?.checked) || (!!root.contextEntry && !!root.menu.ownership[root.contextEntry.id])
                    text: NacreI18n.tr("launcher.app-browser.text.remove-package", "Remove package")
                    onClicked: {
                        const entry = Object.assign({}, root.contextEntry, {
                            disabled: false,
                            operation: "remove-package",
                            value: root.contextEntry.package || root.menu.ownership[root.contextEntry.id]
                        });
                        context.close();
                        root.run(entry);
                    }
                }
                NacreMenuButton {
                    width: parent.width
                    compact: true
                    visible: root.contextEntry?.webapp === true
                    text: NacreI18n.tr("launcher.app-browser.text.remove-web-app", "Remove web app")
                    onClicked: {
                        root.menu.execute(Object.assign({}, root.contextEntry, {
                            operation: "webapp-remove"
                        }));
                        context.close();
                    }
                }
                NacreMenuButton {
                    width: parent.width
                    compact: true
                    visible: !!root.menu.supportPackage(root.contextEntry) && !!root.contextEntry?.disabled
                    text: NacreI18n.tr("launcher.app-browser.text.install-support", "Install support")
                    onClicked: {
                        const pkg = root.menu.supportPackage(root.contextEntry);
                        context.close();
                        root.run({
                            id: "software.support",
                            kind: "action",
                            name: pkg,
                            operation: "install-package",
                            value: pkg
                        });
                    }
                }
                NacreText {
                    width: parent.width
                    text: root.contextEntry?.reason || ""
                    textFormat: Text.PlainText
                    visible: !!text
                    wrapMode: Text.Wrap
                    color: NacreColours.palette.m3onSurfaceVariant
                    font.pointSize: 9
                }
            }
            FastScroll {
                view: contextScroll
                step: 160
            }
        }
        onOpened: contextScroll.contentY = 0
        onClosed: search.forceActiveFocus()
    }

    IpcHandler {
        target: root.detached ? "appBrowserWindow" + root.visibilities.windowKey : "appBrowser"
        function selectCategory(id: string): void {
            root.select(id);
        }
        function openRoute(id: string): void {
            root.navigate(id);
        }
        function state(): string {
            return JSON.stringify({
                route: root.route,
                category: ({
                        home: "favorites",
                        apps: "all"
                    })[root.route] || root.route,
                baseCategory: root.route,
                scope: root.scope.kind,
                calculation: root.calculation,
                count: root.entries.length,
                query: ["ai", "brain"].includes(root.scope.kind) ? root.scope.prefix : root.query,
                columns: root.appLayout ? Math.max(1, Math.floor(grid.width / grid.cellWidth)) : 1,
                width: root.width,
                x: root.mapToItem(null, 0, 0).x,
                y: root.mapToItem(null, 0, 0).y,
                height: root.height,
                visibleRows: root.appLayout ? 5 : Math.floor(results.height / 58),
                currentIndex: root.hasSelection ? root.selection : -1,
                selectedId: root.selectedEntry?.id || "",
                selectedName: root.selectedEntry?.name || "",
                picker: root.picker,
                confirming: !!root.confirmation
            });
        }
    }
}
