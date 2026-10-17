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
    property string resultFilter: "all"
    property string softwareCategory: "all"
    property var softwareDetails: null
    readonly property bool softwareBrowse: !picker && !form && !scope.text && route === "software.discover"
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
    property bool formPending: false
    property var activePrompt: null
    NacreMenuQuery {
        id: nativeQuery
        active: root.visibilities.launcher && !root.picker
        presented: root.visible && !root.picker
        query: root.query
        route: root.route
        resultFilter: root.resultFilter
        category: root.softwareCategory
    }
    readonly property string query: search?.text || ""
    readonly property var scope: Menu.scope(query)
    readonly property bool calculation: scope.kind === "calculator" && root.menu.sourceOptions.calculator
    property bool calculatorAnswerReady: false
    readonly property var calculationResult: {
        if (!calculation)
            return {
                ok: false
            };
        if (NacreDaemon.enabled)
            return nativeQuery.current ? nativeQuery.result.conversion || {
                ok: false
            } : {
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
    readonly property bool showDetails: width >= 940 && !picker && !calculation
    readonly property var entries: {
        if (picker && !inputPicker)
            return Menu.ranked((root.activePrompt.choices || []).map((entry, index) => Object.assign({}, entry, {
                    id: "choice-" + index,
                    kind: "choice",
                    name: entry.label,
                    operation: "choice",
                    icon: entry.icon || "chevron_right"
                })), query);
        if (NacreDaemon.enabled)
            return nativeQuery.entries;
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
        return root.menu.content(route, query, resultFilter, softwareCategory);
    }
    readonly property bool hasSelection: keyboardSelection || !!scope.text || picker
    readonly property var selectedEntry: hasSelection && selection >= 0 ? entries[selection] || null : null
    readonly property var previewEntry: controlsPinned ? null : selectedEntry || hoveredEntry
    onPreviewEntryChanged: root.menu.previews.inspect(previewEntry?.file || previewEntry?.note ? previewEntry.filePath || previewEntry.value : "")
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
        if (!visible || !enabled || (!detached && !visibilities.launcher))
            return;
        search.forceActiveFocus();
    }
    function calculatorStart() {
        const prefix = search.text.indexOf("=") + 1;
        return prefix + search.text.slice(prefix).match(/^\s*/)[0].length;
    }
    function calculatorEdit(value) {
        if (calculatorAnswerReady && /^[0-9.(]/.test(value))
            search.text = "= ";
        let start = Math.max(calculatorStart(), search.selectionStart);
        const end = Math.max(start, search.selectionEnd);
        if (/^[+\-*/]$/.test(value) && start === end && end === search.text.length && /[\d)]\s*[+\-*/]\s*$/.test(scope.text))
            start = search.text.search(/[+\-*/]\s*$/);
        search.remove(start, end);
        search.insert(start, value);
        search.cursorPosition = start + value.length;
        search.forceActiveFocus();
    }
    function calculatorAction(action) {
        if (action === "clear") {
            search.text = "= ";
        } else if (action === "backspace") {
            const prefix = calculatorStart();
            const start = Math.max(prefix, search.selectionStart);
            const end = Math.max(start, search.selectionEnd);
            const removeFrom = start === end ? Math.max(prefix, start - 1) : start;
            search.remove(removeFrom, end);
            search.cursorPosition = removeFrom;
            search.forceActiveFocus();
            return;
        } else if (action === "equals") {
            // Unit conversions can return a labelled value, not an arithmetic operand.
            if (calculationResult.ok && Calculator.evaluate(calculationResult.value).ok) {
                search.text = "= " + calculationResult.value;
                calculatorAnswerReady = true;
            }
        } else if (action === "copy") {
            if (calculationResult.ok)
                AppLaunch.run(["wl-copy", "--", calculationResult.value]);
        } else if (action === "sign" && scope.text) {
            search.text = "= -(" + scope.text + ")";
        } else if (action === "square" && scope.text) {
            search.text = "= (" + scope.text + ")^2";
        } else if (action === "sqrt") {
            if (scope.text) {
                const operand = "sqrt(" + scope.text + ")";
                search.text = "= " + operand;
            } else
                calculatorEdit("sqrt(");
        }
        search.cursorPosition = search.text.length;
        search.forceActiveFocus();
    }
    function captureState() {
        return {
            route: route,
            resultFilter: resultFilter,
            softwareCategory: softwareCategory,
            query: query,
            calculatorAnswerReady: calculation && calculatorAnswerReady,
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
        softwareCategory = data.softwareCategory || "all";
        resultFilter = root.menu.searchFilters.some(row => row.value === data.resultFilter) ? data.resultFilter : "all";
        history = data.history || [];
        search.text = data.query || "";
        calculatorAnswerReady = data.calculatorAnswerReady === true && calculation;
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
        resultFilter = "all";
        softwareCategory = "all";
        softwareDetails = null;
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
        root.focusExplicit();
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
        calculatorAnswerReady = false;
        resultFilter = "all";
        softwareCategory = "all";
        softwareDetails = null;
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
                button: NacreI18n.tr("ui.app-browser.button.continue", "Continue")
            };
        }
        Qt.callLater(root.focusExplicit);
    }
    function back() {
        if (softwareDetails) {
            softwareDetails = null;
            return;
        }
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
    function primaryAction(entry) {
        if (!entry)
            return "";
        if (entry.operation === "webapp-offer")
            return NacreI18n.tr("software.ai.web-offer", "Create web app");
        if (entry.theme)
            return NacreI18n.tr("software.preview.apply", "Apply theme");
        if (entry.operation === "software-extension-install")
            return NacreI18n.tr("software.action.install-extension", "Install extension");
        if (entry.operation === "software-extension-toggle")
            return entry.extension?.enabled ? NacreI18n.tr("software.action.disable", "Disable extension") : NacreI18n.tr("software.action.enable", "Enable extension");
        if (entry.operation === "software-extension-open")
            return NacreI18n.tr("software.action.open-panel", "Open panel");

        if (entry?.kind === "app")
            return NacreI18n.tr("launcher.app-browser.text.open-app", "Open app");
        if (entry?.file)
            return NacreI18n.tr("launcher.workflows.open-file", "Open file");
        if (entry?.note)
            return NacreI18n.tr("launcher.workflows.open-note", "Open note");
        if (entry?.clipboard || entry?.emoji || entry?.workflow?.type === "snippet")
            return NacreI18n.tr("launcher.workflows.copy-item", "Copy to clipboard");
        if (entry?.operation === "focus")
            return NacreI18n.tr("launcher.workflows.focus", "Switch to window");
        if (entry?.operation === "workflow")
            return NacreI18n.tr("launcher.workflows.open-saved", "Open saved item");
        return NacreI18n.tr("launcher.app-browser.text.run", "Run");
    }
    function softwareForm(entry) {
        const kind = entry.value;
        if (kind === "software-extension-import")
            return {
                kind: kind,
                software: "import",
                title: NacreI18n.tr("software.form.add-extension", "Add an extension"),
                description: NacreI18n.tr("software.form.import-description", "Import a Nacre extension repository. It stays disabled until you review and enable it."),
                fields: [
                    {
                        id: "url",
                        label: NacreI18n.tr("software.form.repository", "Repository URL"),
                        placeholder: "https://github.com/owner/extension"
                    }
                ],
                button: NacreI18n.tr("software.form.import", "Import extension")
            };
        if (kind === "software-clone")
            return {
                kind: kind,
                software: "clone",
                value: entry.sourceExtension,
                title: NacreI18n.tr("software.form.clone", "Clone extension to edit"),
                fields: [
                    {
                        id: "id",
                        label: NacreI18n.tr("software.form.id", "Your extension ID"),
                        placeholder: "user.my-extension"
                    },
                    {
                        id: "name",
                        label: NacreI18n.tr("software.form.name", "Name")
                    }
                ],
                button: NacreI18n.tr("software.form.create-clone", "Create clone")
            };
        return {
            kind: kind,
            software: kind === "software-theme-export" ? "theme-export" : "theme-import",
            value: entry.themePreset || "",
            title: kind === "software-theme-export" ? NacreI18n.tr("software.form.export-theme", "Export theme") : NacreI18n.tr("software.form.import-theme", "Import theme"),
            fields: [
                {
                    id: "path",
                    label: NacreI18n.tr("software.form.theme-file", "Theme manifest file"),
                    placeholder: kind === "software-theme-export" ? NacreI18n.tr("software.form.export-path", "{home}/Documents/my-theme.json", {
                        home: NacrePaths.home
                    }) : NacreI18n.tr("software.form.import-path", "{home}/Downloads/nacre.extension.json", {
                        home: NacrePaths.home
                    })
                }
            ],
            button: kind === "software-theme-export" ? NacreI18n.tr("software.form.export", "Export") : NacreI18n.tr("software.form.import-theme-button", "Import theme")
        };
    }
    function workflowForm(kind) {
        const field = function (id, label, initial, multiline) {
            return {
                id: id,
                label: label,
                initial: initial || "",
                multiline: multiline === true
            };
        };
        const label = field("label", NacreI18n.tr("launcher.forms.name", "Name"));
        const icon = field("icon", NacreI18n.tr("launcher.forms.icon", "Icon"), "terminal");
        const folder = field("directory", NacreI18n.tr("launcher.forms.folder", "Working directory"));
        const definitions = {
            action: {
                title: NacreI18n.tr("launcher.forms.action", "Add launcher action"),
                fields: [label, icon, field("executable", NacreI18n.tr("launcher.forms.executable", "Executable")), field("arguments", NacreI18n.tr("launcher.forms.arguments", "Arguments (one per line)"), "", true), folder]
            },
            snippet: {
                title: NacreI18n.tr("launcher.forms.snippet", "Save a text snippet"),
                fields: [label, field("text", NacreI18n.tr("launcher.forms.text", "Text"), "", true)]
            },
            search: {
                title: NacreI18n.tr("launcher.forms.search", "Add a named web search"),
                fields: [label, field("url", NacreI18n.tr("launcher.forms.search-url", "Search URL with {query}"), "https://www.google.com/search?q={query}")]
            },
            bookmark: {
                title: NacreI18n.tr("launcher.forms.bookmark", "Save a bookmark"),
                fields: [label, field("url", NacreI18n.tr("launcher.forms.url", "Website URL"), "https://")]
            },
            workspace: {
                title: NacreI18n.tr("launcher.forms.workspace", "Save a workspace recipe"),
                fields: [label, folder, field("editor", NacreI18n.tr("launcher.forms.editor", "Editor command (one argument per line)"), "code\n--new-window\n--wait\n{project}", true), field("terminal", NacreI18n.tr("launcher.forms.terminal", "Terminal command (one argument per line)"), "kitty\n--directory\n{project}", true), field("url", NacreI18n.tr("launcher.forms.docs", "Documentation URL (optional)"))]
            },
            roots: {
                title: NacreI18n.tr("launcher.forms.roots", "Choose file search folders"),
                fields: [field("roots", NacreI18n.tr("launcher.forms.roots-list", "Extra folders (one per line)"), (root.menu.workflows.data.roots || []).join("\n"), true)]
            }
        };
        return Object.assign({
            kind: kind,
            workflow: true,
            description: NacreI18n.tr("launcher.forms.private", "Saved privately. Normal launcher search finds these items automatically."),
            button: NacreI18n.tr("launcher.forms.save", "Save")
        }, definitions[kind]);
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
        // Presentation data does not authorize activation. Context actions with
        // a changed row ID cannot inherit the original row's engine ticket.
        if (NacreDaemon.enabled && entry.daemonTicket && entry.id === entry.daemonRowId && entry.operation === entry.daemonOperation) {
            nativeQuery.activate(entry, (value, failure) => {
                if (failure) {
                    root.menu.message = failure.message;
                    return;
                }
                if (value.view?.route !== undefined)
                    root.navigate(value.view.route);
                if (value.view?.query !== undefined)
                    search.text = value.view.query;
                if (!value.keep_open && (!root.detached || root.closeAfterLaunch)) {
                    root.visibilities.launcher = false;
                    if (root.detached)
                        root.closeRequested();
                }
            });
            return;
        }
        if (entry.operation === "scope") {
            search.text = entry.value;
            root.focusExplicit();
            return;
        }
        if (entry.operation === "calculator") {
            search.text = "= ";
            root.focusExplicit();
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
        if (entry.operation === "tool" && ["record-start", "record-screen"].includes(entry.value)) {
            form = {
                kind: "recording",
                screen: entry.value === "record-screen",
                title: entry.name,
                description: NacreI18n.tr("launcher.capture.devices", "Uses your current audio output and microphone. Muted devices stay muted."),
                fields: [
                    {
                        id: "audio",
                        label: NacreI18n.tr("launcher.capture.sound", "Recording sound"),
                        initial: "none",
                        choices: [
                            {
                                value: "none",
                                label: NacreI18n.tr("launcher.capture.no-sound", "No sound")
                            },
                            {
                                value: "system",
                                label: NacreI18n.tr("launcher.capture.system", "System audio")
                            },
                            {
                                value: "mixed",
                                label: NacreI18n.tr("launcher.capture.mixed", "System audio and microphone")
                            }
                        ]
                    }
                ],
                button: NacreI18n.tr("launcher.capture.start", "Start recording")
            };
            return;
        }
        if (entry.operation === "software-preview" || entry.bundle && !entry.bundleScope) {
            softwareDetails = entry;
            return;
        }
        if (entry.operation === "form" && entry.value.startsWith("software-")) {
            form = softwareForm(entry);
            return;
        }
        if (entry.operation === "form" && entry.value === "tui") {
            form = {
                kind: "tui",
                software: "tui-create",
                title: NacreI18n.tr("launcher.catalog.software.tui.new.label", "Create a TUI app"),
                description: NacreI18n.tr("software.tui.argv-help", "Enter one command argument per line. Creation saves an entry without running it."),
                fields: [
                    {
                        id: "name",
                        label: NacreI18n.tr("software.tui.name", "App name")
                    },
                    {
                        id: "command",
                        label: NacreI18n.tr("software.tui.command", "Command arguments"),
                        multiline: true,
                        initial: "htop"
                    },
                    {
                        id: "icon",
                        label: NacreI18n.tr("software.tui.icon", "Icon name"),
                        initial: "utilities-terminal"
                    },
                    {
                        id: "terminal",
                        label: NacreI18n.tr("software.tui.terminal", "Terminal"),
                        initial: "kitty",
                        choices: [
                            {
                                value: "kitty",
                                label: NacreI18n.tr("software.tui.kitty", "Kitty")
                            },
                            {
                                value: "foot",
                                label: NacreI18n.tr("software.tui.foot", "Foot")
                            },
                            {
                                value: "alacritty",
                                label: NacreI18n.tr("software.tui.alacritty", "Alacritty")
                            },
                            {
                                value: "wezterm",
                                label: NacreI18n.tr("software.tui.wezterm", "WezTerm")
                            }
                        ]
                    }
                ],
                button: NacreI18n.tr("ui.app-browser.button.create-app", "Create app")
            };
            return;
        }
        if (entry.operation === "form") {
            if (["action", "snippet", "search", "bookmark", "workspace", "roots"].includes(entry.value)) {
                form = workflowForm(entry.value);
                return;
            }
            form = entry.value === "webapp" ? {
                kind: "webapp",
                title: NacreI18n.tr("launcher.app-browser.title.create-a-web-app", "Create a web app"),
                description: NacreI18n.tr("launcher.app-browser.description.add-a-website-to-all-apps", "Add a website to All apps."),
                fields: [
                    {
                        id: "label",
                        label: NacreI18n.tr("launcher.app-browser.label.app-name", "App name"),
                        placeholder: NacreI18n.tr("launcher.app-browser.placeholder.my-website", "My website")
                    },
                    {
                        id: "url",
                        label: NacreI18n.tr("launcher.app-browser.label.website", "Website"),
                        placeholder: "https://…"
                    }
                ],
                button: NacreI18n.tr("ui.app-browser.button.create-app", "Create app")
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
                        placeholder: NacreI18n.tr("launcher.app-browser.placeholder.time-for-a-break", "Time for a break")
                    }
                ],
                button: NacreI18n.tr("ui.app-browser.button.set-reminder", "Set reminder")
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
        root.focusExplicit();
    }
    Timer {
        id: hoverClear
        interval: 160
        onTriggered: if (!detailHover.hovered)
            root.hoveredEntry = null
    }
    function focusResults(delta = 1) {
        if (calculation) {
            calculatorPad.item?.focusFirst();
            return;
        }
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
            if (calculation) {
                event.accepted = false;
                return;
            }
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
            root.focusExplicit();
            event.accepted = true;
        } else if (event.key === Qt.Key_Backspace) {
            query ? search.text = search.text.slice(0, -1) : back();
            root.focusExplicit();
            event.accepted = true;
        }
    }
    function dismiss() {
        if (context.opened) {
            context.close();
            root.focusExplicit();
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
        root.focusExplicit();
    }
    function submitForm(values) {
        if (formPending)
            return;
        if (inputPicker)
            NacreMenuPrompt.finish(values.value || "", false);
        else if (form.kind === "recording") {
            const options = Object.assign({}, values, {
                screen: form.screen
            });
            if (!detached || closeAfterLaunch)
                visibilities.launcher = false;
            root.menu.workflows.startRecording(options);
        } else if (form.software) {
            const payload = form.kind === "tui" ? Object.assign({}, values, {
                command: (values.command || "").split("\n").filter(line => line.length > 0)
            }) : values;
            formPending = root.menu.library.work(form.software, form.value || "", payload);
            return;
        } else if (form.workflow) {
            formPending = root.menu.workflows.work("save", form.kind, values);
            return;
        } else
            root.menu.work(form.kind === "webapp" ? "webapp-create" : "reminder", "", values);
        form = null;
        root.focusExplicit();
    }
    onQueryChanged: {
        calculatorAnswerReady = false;
        controlsPinned = false;
        keyboardSelection = false;
        hoveredEntry = null;
        selection = scope.text || picker ? Menu.firstEnabled(entries) : -1;
        queryDelay.restart();
    }
    property string selectedStableId: ""
    onKeyboardSelectionChanged: if (keyboardSelection && entries[selection])
        selectedStableId = Menu.stable(entries[selection])
    onSelectionChanged: if (keyboardSelection && entries[selection])
        selectedStableId = Menu.stable(entries[selection])
    onEntriesChanged: {
        const previous = keyboardSelection ? entries.findIndex(entry => Menu.stable(entry) === selectedStableId) : -1;
        if (previous >= 0)
            selection = previous;
        else if (scope.text || picker)
            selection = Menu.firstEnabled(entries);
        else if (!keyboardSelection)
            selection = -1;
        else if (selection >= entries.length)
            selection = entries.length - 1;
    }
    Connections {
        target: root.menu.library
        function onCompleted(operation, success) {
            if (root.form?.software && root.formPending) {
                root.formPending = false;
                if (success)
                    root.form = null;
                root.focusExplicit();
            }
        }
    }
    Component.onCompleted: reset()
    Connections {
        target: root.visibilities
        function onLauncherChanged() {
            if (root.visibilities.launcher)
                root.reset();
            else
                search.focus = false;
        }
        function onLauncherRequestChanged() {
            if (root.visibilities.launcher)
                Qt.callLater(root.reset);
        }
    }
    Connections {
        target: root.menu.workflows
        function onCompleted(success) {
            if (root.formPending) {
                root.formPending = false;
                if (success)
                    root.cancelForm();
            }
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
                            color: NacreTokens.gradientColour(NacreColours.palette.m3primary, NacreColours.palette.m3secondary)
                        }
                        GradientStop {
                            position: 1
                            color: NacreTokens.gradientColour(NacreColours.palette.m3primary, NacreColours.palette.m3tertiary)
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
    NacreMenuButton {
        id: filterButton
        objectName: "launcherSearchFilter"
        x: root.contentX + root.contentWidth - width
        y: 48
        width: 135
        height: 28
        compact: true
        visible: root.scope.kind === "all" && !!root.scope.text && !root.picker && !root.form
        text: root.menu.searchFilters.find(item => item.value === root.resultFilter)?.label || ""
        icon: "filter_list"
        onClicked: filters.open()
        Popup {
            id: filters
            y: filterButton.height + 4
            width: filterButton.width + 20
            padding: 6
            background: NacreSurface {
                color: NacreColours.palette.m3surfaceContainer
                radius: 10
            }
            contentItem: Column {
                spacing: 3
                Repeater {
                    model: root.menu.searchFilters
                    delegate: NacreMenuButton {
                        required property var modelData
                        width: parent.width
                        compact: true
                        text: modelData.label
                        selected: root.resultFilter === modelData.value
                        onClicked: {
                            root.resultFilter = modelData.value;
                            root.keyboardSelection = false;
                            root.selection = Menu.firstEnabled(root.entries);
                            filters.close();
                            search.forceActiveFocus();
                        }
                    }
                }
            }
        }
    }
    NacreMenuButton {
        objectName: "launcherStopRecording"
        anchors.right: parent.right
        anchors.rightMargin: 20
        y: 16
        visible: root.menu.workflows.data.recording === true && !root.picker
        compact: true
        icon: "stop_circle"
        text: NacreI18n.tr("launcher.capture.stop-elapsed", "Stop recording {time}", {
            time: root.menu.workflows.elapsed
        })
        onClicked: root.menu.workflows.work("record-stop")
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
                fontSizeMode: Text.Fit
                minimumPointSize: 8
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
    Flickable {
        objectName: "softwareCategoryStrip"
        visible: root.softwareBrowse && !root.softwareDetails
        x: root.contentX
        y: 84
        width: root.contentWidth
        height: 34
        clip: true
        contentWidth: softwareCategories.width
        contentHeight: height
        flickableDirection: Flickable.HorizontalFlick
        boundsBehavior: Flickable.StopAtBounds
        Row {
            id: softwareCategories
            spacing: 5
            Repeater {
                model: root.menu.softwareCategories
                NacreMenuButton {
                    required property var modelData
                    compact: true
                    text: modelData.label
                    selected: root.softwareCategory === modelData.value
                    onClicked: {
                        root.softwareCategory = modelData.value;
                        root.selection = -1;
                    }
                }
            }
        }
    }
    ListView {
        id: results
        objectName: "launcherResults"
        visible: !root.appLayout && !root.form && !root.calculation
        x: root.contentX
        y: root.picker ? 54 : root.softwareBrowse ? 126 : 84
        width: root.contentWidth
        height: Math.floor((root.bodyHeight - (root.softwareBrowse ? 42 : 0)) / 58) * 58 + (root.picker ? 30 : 0)
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
        visible: !root.entries.length && !root.form && !root.calculation
        text: root.calculation ? root.calculationResult.error || NacreI18n.tr("launcher.app-browser.text.type-an-expression-after", "Type an expression after =") : root.scope.kind === "files" ? root.menu.fileMessage || NacreI18n.tr("launcher.app-browser.text.type-and-a-filename-to-search-your-folders", "Type : and a filename to search your folders") : root.scope.kind === "brain" ? (root.scope.text ? NacreI18n.tr("launcher.app-browser.text.looking-in-your-private-notes", "Looking in your private notes…") : NacreI18n.tr("launcher.app-browser.text.type-and-a-few-words-to-search-nacre-brain", "Type # and a few words to search Nacre Brain")) : root.scope.kind === "ai" ? NacreI18n.tr("launcher.app-browser.text.type-and-your-question", "Type ? and your question") : root.route === "home" && !root.query ? NacreI18n.tr("launcher.app-browser.text.add-apps-and-menu-items-to-favorites-with-ctrl-d-or-the", "Add apps and menu items to Favorites with Ctrl+D or their item actions.") : root.route === "software.packages" ? root.menu.packageMessage || NacreI18n.tr("launcher.app-browser.text.type-two-or-more-characters-to-search-packages", "Type two or more characters to search packages") : NacreI18n.tr("launcher.app-browser.text.no-matches-try-a-different-word-for-actions-or-for-sear", "No matches. Try a different word, > for actions, or / for search scopes.")
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
        color: NacreColours.palette.m3onSurfaceVariant
        font.pointSize: 11
    }
    Loader {
        id: calculatorPad
        objectName: "launcherCalculatorLoader"
        visible: root.calculation && !root.form && !root.picker
        active: visible
        width: Math.min(460, root.contentWidth)
        height: Math.max(0, root.bodyHeight)
        x: root.contentX + (root.contentWidth - width) / 2
        y: 84
        sourceComponent: NacreCalculatorPad {
            objectName: "launcherCalculator"
            expression: root.scope.text
            result: root.calculationResult
            onInsertRequested: value => root.calculatorEdit(value)
            onActionRequested: action => root.calculatorAction(action)
        }
    }
    Loader {
        objectName: "softwarePreviewLoader"
        active: !!root.softwareDetails
        visible: active
        x: root.contentX
        y: 84
        width: root.width - root.contentX - 20
        height: root.bodyHeight
        z: 25
        sourceComponent: NacreSoftwarePreview {
            entry: root.softwareDetails || ({})
            mode: NacreWallpapers.appearancePreferences.paletteMode || "dark"
            onCloseRequested: {
                root.softwareDetails = null;
                root.focusExplicit();
            }
            onBundleApplyRequested: (mode, scope) => {
                root.run(Object.assign({}, root.softwareDetails, {
                    operation: "software-theme",
                    themeMode: mode,
                    bundleScope: scope
                }));
                root.softwareDetails = null;
            }
            onApplyRequested: mode => {
                root.run(Object.assign({}, root.softwareDetails, {
                    operation: "software-theme",
                    themeMode: mode
                }));
                root.softwareDetails = null;
            }
        }
    }
    NacreSurface {
        id: details
        visible: root.showDetails && !root.form && !root.softwareDetails
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
                    visible: !!root.previewEntry?.screenshots?.length || !!root.previewEntry?.preview || !!root.menu.previews.preview.image && root.menu.previews.previewPath === (root.previewEntry?.filePath || root.previewEntry?.value)
                    source: visible ? root.previewEntry?.screenshots?.[0] || root.previewEntry?.preview || root.menu.previews.preview.image : ""
                    sourceSize.width: 512
                    sourceSize.height: 288
                    asynchronous: true
                    fillMode: Image.PreserveAspectFit
                }
                NacreThemePreview {
                    width: parent.width
                    visible: root.previewEntry?.theme === true
                    entry: root.previewEntry || ({})
                    mode: NacreWallpapers.appearancePreferences.paletteMode || "dark"
                }
                NacreText {
                    width: parent.width
                    visible: root.previewEntry?.software === true || !!root.previewEntry?.extension
                    text: [root.previewEntry?.categoryLabel, root.previewEntry?.repository, root.previewEntry?.version, root.previewEntry?.status].filter(Boolean).join(" · ")
                    textFormat: Text.PlainText
                    font.pointSize: 9
                    wrapMode: Text.Wrap
                    color: NacreColours.palette.m3onSurfaceVariant
                }
                NacreMenuButton {
                    width: parent.width
                    visible: root.previewEntry?.theme === true || !!root.previewEntry?.screenshots?.length
                    text: NacreI18n.tr("software.preview.more", "View previews")
                    icon: "image"
                    onClicked: root.softwareDetails = root.previewEntry
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
                    text: root.previewEntry?.operation === "wallpaper" ? NacreI18n.tr("launcher.app-browser.text.use-wallpaper", "Use wallpaper") : root.previewEntry?.operation === "font" ? NacreI18n.tr("launcher.app-browser.text.use-font", "Use font") : root.previewEntry?.operation === "install-package" ? NacreI18n.tr("launcher.app-browser.text.install", "Install") : root.primaryAction(root.previewEntry)
                    onClicked: root.run(root.previewEntry)
                }
                NacreMenuButton {
                    width: parent.width
                    visible: root.previewEntry?.software === true && root.previewEntry?.checked === true && root.menu.packageNames.has(root.previewEntry.package)
                    text: NacreI18n.tr("software.action.updates", "Check system updates")
                    icon: "update"
                    onClicked: root.run(Object.assign({}, root.previewEntry, {
                        kind: "action",
                        operation: "tool",
                        value: "updates"
                    }))
                }
                NacreMenuButton {
                    width: parent.width
                    visible: root.previewEntry?.software === true && root.previewEntry?.checked === true && root.menu.packageNames.has(root.previewEntry.package)
                    text: NacreI18n.tr("software.action.remove", "Remove app")
                    icon: "delete"
                    onClicked: root.run(Object.assign({}, root.previewEntry, {
                        kind: "action",
                        operation: "remove-package",
                        value: root.previewEntry.package,
                        confirm: NacreI18n.tr("software.action.remove-confirm", "Open the package manager to remove this app?")
                    }))
                }
                NacreMenuButton {
                    width: parent.width
                    visible: !!root.previewEntry?.website
                    text: NacreI18n.tr("software.action.website", "Project website")
                    icon: "open_in_new"
                    onClicked: AppLaunch.run(["xdg-open", root.previewEntry.website])
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
        text: (NacreDaemon.enabled && NacreDaemon.stale ? NacreI18n.tr("helper.sidebar-chat.reconnecting", "Reconnecting…") : "") || nativeQuery.error || root.menu.catalog.error || root.menu.library.message || root.menu.workflows.message || root.menu.message || (root.calculation ? NacreI18n.tr("launcher.app-browser.text.enter-copies-the-result", "Enter copies the result") : root.picker ? NacreI18n.tr("launcher.app-browser.text.enter-chooses-escape-cancels", "Enter chooses · Escape cancels") : NacreI18n.tr("launcher.app-browser.text.enter-opens-tab-item-actions-ctrl-d-favorite", "Enter opens · Tab item actions · Ctrl+D favorite"))
        textFormat: Text.PlainText
        font.pointSize: 9
        color: NacreColours.palette.m3onSurfaceVariant
        elide: Text.ElideRight
        visible: !root.form && !root.confirmation && (!!root.selectedEntry || root.calculation || root.picker || !!root.menu.catalog.error || !!root.menu.message || !!root.menu.workflows.message || !!root.menu.library.message)
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
        persistentSelection: root.calculation
        onAccepted: root.activate(root.selection)
        Keys.onDownPressed: root.focusResults()
        Keys.onEscapePressed: root.dismiss()
        Keys.onPressed: event => {
            if (event.key === Qt.Key_D && event.modifiers & Qt.ControlModifier) {
                root.favorite(root.selectedEntry);
                event.accepted = true;
            } else if (event.key === Qt.Key_Tab) {
                if (!root.calculation) {
                    root.selectedEntry ? root.actionsFor(root.selectedEntry) : rail.forceActiveFocus();
                    event.accepted = true;
                } else
                    event.accepted = false;
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
            model: [["actions", "> Actions"], ["calculator", "= Calculate"], ["brain", "# Brain"], ["files", ": Files"], ["ai", "? AI"]].filter(row => root.menu.sourceOptions[row[0]]).map(row => row[1])
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
            error: root.form?.software ? root.menu.library.message : root.form?.workflow ? root.menu.workflows.message : ""
            enabled: !root.formPending
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
                        root.focusExplicit();
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
                    visible: !!root.contextEntry && !root.contextEntry.transient && !["open-file", "search-web"].includes(root.contextEntry.operation)
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
                Repeater {
                    model: root.menu.itemActions(root.contextEntry)
                    delegate: NacreMenuButton {
                        required property var modelData
                        width: parent.width
                        text: modelData.name
                        onClicked: {
                            root.run(modelData);
                            context.close();
                        }
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
