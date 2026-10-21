import QtQuick
import QtTest
import "calculator.js" as Arithmetic
import "fixtures"

TestCase {
    id: test

    function init() {
        mouseMove(test, width - 2, height - 2);
        LauncherPreferences.favorites = [];
        LauncherPreferences.hidden = [];
        LauncherPreferences.recent = [];
        NacreApps.launched = "";
        AppLaunch.commands = [];
        SidebarChat.commands = [];
        SidebarChat.busy = false;
        SidebarChat.draft = "";
        NacreMenu.snapshot = {
            "version": 1,
            "capabilities": {
                "capture": true,
                "ai": true,
                "brain": true,
                "packages": true
            },
            "packages": [],
            "fonts": ["IBM Plex Sans"],
            "reminders": [],
            "webapps": []
        };
        NacreMenu.localSourceOptions = {
            version: 1,
            brain: true,
            files: true,
            calculator: true,
            actions: true,
            ai: true,
            webEngine: "duckduckgo"
        };
        NacreMenu.message = "";
        NacreMenu.library.data = {
            extensions: [],
            themes: []
        };
        NacreMenu.library.message = "";
        const softwareWorker = findChild(NacreMenu.library, "softwareLibraryAction");
        if (softwareWorker)
            softwareWorker.running = false;
        const emojiWorker = findChild(NacreMenu.previews, "launcherEmojiSearch");
        if (emojiWorker)
            emojiWorker.running = false;
        NacreMenu.previews.searchEmoji("");
        NacreMenu.workflows.data = {
            entries: [],
            projects: [],
            captures: [],
            recording: false
        };
        DesktopExtras.clips = [];
        NacreCompositor.windows = [];
        NacreCompositor.backendName = "hyprland";
        NacrePanelState.calls = [];
    }

    function view() {
        const value = createTemporaryObject(scene, this);
        wait(40);
        return value;
    }

    function test_hidden_or_disabled_launcher_cannot_force_search_focus() {
        const value = view();
        const search = findChild(value, "launcherSearch");
        verify(search);
        value.visible = false;
        search.focus = false;
        value.focusExplicit();
        compare(search.focus, false);
        value.visible = true;
        value.enabled = false;
        search.focus = false;
        value.focusExplicit();
        compare(search.focus, false);
        value.enabled = true;
        value.focusExplicit();
        compare(search.focus, true);
    }

    function test_search_receives_focus_on_fresh_open_and_reopen() {
        const value = view();
        const search = findChild(value, "launcherSearch");
        verify(search);
        tryCompare(search, "activeFocus", true);
        keyClick(Qt.Key_E);
        compare(search.text, "e");

        value.visibilities.launcher = false;
        search.focus = false;
        value.visibilities.launcher = true;
        tryCompare(search, "activeFocus", true);
        keyClick(Qt.Key_D);
        compare(search.text, "d");
    }

    function test_ordinary_typing_federates_providers_and_filters_are_optional() {
        const value = view(), field = findChild(value, "launcherSearch");
        value.navigate("power");
        NacreMenu.fileQuery = "report";
        NacreMenu.fileRows = [
            {
                id: "file",
                name: "Report.txt",
                path: "/fixture/Report.txt"
            }
        ];
        NacreMenu.brainQuery = "report";
        NacreMenu.notes = [
            {
                path: "report.md",
                title: "Report notes",
                preview: "Report context"
            }
        ];
        DesktopExtras.clips = [
            {
                id: "41",
                title: "Report copied text",
                image: false,
                pinned: true
            }
        ];
        NacreCompositor.windows = [
            {
                windowId: "9",
                title: "Report window",
                wmClass: "editor",
                workspace: {
                    name: "1"
                }
            }
        ];
        NacreMenu.workflows.data = {
            entries: [
                {
                    id: "saved",
                    label: "Report template",
                    type: "snippet",
                    text: "Report body"
                }
            ],
            projects: [],
            captures: []
        };
        field.text = "report";
        wait(40);
        compare(value.resultFilter, "all");
        for (const operation of ["open-file", "note", "copy-clip", "focus", "workflow"])
            verify(value.entries.some(row => row.operation === operation), operation);
        verify(value.entries.find(row => row.operation === "copy-clip").pinned);
        compare(value.entries.find(row => row.operation === "copy-clip").previewText, "Report copied text");
        value.resultFilter = "files";
        compare(value.entries.length, 1);
        compare(value.entries[0].operation, "open-file");
        value.reset();
        compare(value.resultFilter, "all");
        field.text = "report";
        value.menu.prepare(value.route, value.query);
        compare(value.menu.fileWanted, "report");
        compare(value.menu.brainWanted, "report");
        field.text = "r";
        value.menu.prepare(value.route, value.query);
        compare(value.menu.fileWanted, "");
        compare(value.menu.brainWanted, "");
    }
    function test_clipboard_images_and_contextual_actions_keep_producer_fields() {
        DesktopExtras.clips = [
            {
                id: "42",
                title: "Copied image",
                image: true,
                thumbnail: "/fixture/clip.png",
                pinned: false
            }
        ];
        const row = NacreMenu.provider("actions.clipboard")[0];
        compare(row.name, "Copied image");
        compare(row.preview, "/fixture/clip.png");
        verify(NacreMenu.itemActions(row).some(action => action.operation === "clip-pin"));
        NacreMenu.execute(NacreMenu.itemActions(row).find(action => action.operation === "clip-pin"));
        const latest = DesktopExtras.requests[DesktopExtras.requests.length - 1];
        compare(latest[0], "pin");
        compare(latest[1].id, "42");
        NacreCompositor.windows = [
            {
                windowId: "9",
                title: "Working document",
                wmClass: "editor"
            }
        ];
        const switchAction = NacreMenu.itemActions(NacreMenu.apps.find(row => row.id === "editor")).find(row => row.operation === "focus");
        verify(switchAction !== undefined);
        compare(switchAction.value, "9");
    }
    function test_async_results_preserve_keyboard_selection() {
        const value = view(), field = findChild(value, "launcherSearch");
        field.text = "editor";
        value.focusResults();
        const id = value.selectedEntry.id;
        NacreMenu.fileQuery = "editor";
        NacreMenu.fileRows = [
            {
                id: "late",
                name: "Editor notes.md",
                path: "/fixture/editor.md"
            }
        ];
        compare(value.selectedEntry.id, id);
    }
    function test_private_workflow_forms_scroll_at_short_height() {
        const value = view();
        value.height = 400;
        value.run(NacreMenu.node("actions.saved.add-workspace"));
        compare(value.form.kind, "workspace");
        const last = findChild(value, "menuFormField_url");
        verify(last !== null);
        const flick = last.parent.parent.parent.parent;
        wait(40);
        verify(flick.contentHeight > flick.height);
        value.cancelForm();
        value.run(NacreMenu.node("actions.saved.add-snippet"));
        verify(value.form.fields.some(row => row.multiline));
    }
    Component {
        id: compositorWindow
        QtObject {
            property var record: ({
                    windowId: "qa-window",
                    title: "Discord documentation",
                    wmClass: "browser",
                    workspace: {
                        name: "1"
                    },
                    geometry: {
                        x: 0
                    }
                })
            readonly property string windowId: record.windowId
            readonly property string title: record.title
            readonly property string wmClass: record.wmClass
            readonly property var workspace: record.workspace
        }
    }
    function test_unchanged_window_and_clipboard_snapshots_do_not_rebuild_search() {
        const value = view(), field = findChild(value, "launcherSearch");
        const window = createTemporaryObject(compositorWindow, test);
        NacreCompositor.windows = [window];
        DesktopExtras.clips = [
            {
                id: "qa-clip",
                title: "Discord clipboard",
                image: false
            }
        ];
        field.text = "disc";
        const focus = value.entries.findIndex(row => row.operation === "focus");
        verify(focus >= 0);
        verify(value.entries.some(row => row.operation === "copy-clip"));
        value.keyboardSelection = true;
        value.selection = focus;
        const original = value.entries;
        for (let index = 0; index < 25; index++) {
            window.record = Object.assign({}, window.record, {
                workspace: {
                    name: "1"
                },
                geometry: {
                    x: index
                }
            });
            NacreCompositor.windows = [window];
            DesktopExtras.clips = [
                {
                    id: "qa-clip",
                    title: "Discord clipboard",
                    image: false
                }
            ];
            verify(value.entries === original, "Unchanged search text must preserve its result model");
            compare(value.selectedEntry.value, "qa-window");
        }
        window.record = Object.assign({}, window.record, {
            title: "Discord revised documentation"
        });
        compare(value.selectedEntry.name, "Discord revised documentation");
        DesktopExtras.clips = [
            {
                id: "qa-clip",
                title: "Updated Discord clipboard",
                image: false
            }
        ];
        compare(value.entries.find(row => row.operation === "copy-clip").name, "Updated Discord clipboard");
    }

    function test_warm_search_cache_matches_cold_results_and_tracks_changes() {
        const value = view(), field = findChild(value, "launcherSearch");
        NacreMenu.searchCacheWanted = false;
        NacreMenu.workflows.data = {
            entries: [],
            projects: [],
            captures: [{ name: "Zephyr capture", path: "/tmp/zephyr.png" }],
            recording: false
        };
        field.text = "zephyr";
        wait(20);
        const cold = value.entries.map(row => row.id);
        verify(cold.length > 0, "The capture row must be searchable");
        NacreMenu.searchCacheWanted = true;
        wait(20);
        compare(value.entries.map(row => row.id), cold, "A warm corpus must rank exactly like a cold one");
        NacreMenu.workflows.data = {
            entries: [],
            projects: [],
            captures: [{ name: "Quasar capture", path: "/tmp/quasar.png" }],
            recording: false
        };
        field.text = "quasar";
        wait(20);
        verify(value.entries.some(row => row.value === "/tmp/quasar.png"), "Changed source data must invalidate the warm corpus");
        field.text = "zephyr";
        wait(20);
        verify(!value.entries.some(row => row.value === "/tmp/zephyr.png"));
        NacreMenu.searchCacheWanted = false;
    }

    function test_software_categories_are_optional_and_global_typing_stays_default() {
        const value = view(), field = findChild(value, "launcherSearch");
        value.navigate("software.discover");
        verify(findChild(value, "softwareCategoryStrip").visible);
        value.softwareCategory = "creative";
        verify(value.entries.length > 0);
        verify(value.entries.every(row => row.category === "creative"));
        field.text = "chrome";
        verify(!findChild(value, "softwareCategoryStrip").visible);
        compare(value.entries[0].kind, "app");
        compare(value.entries[0].name, "Google Chrome");
        value.navigate("software.themes");
        field.text = "obs";
        verify(value.entries.some(row => row.package === "obs-studio"));
    }
    function test_software_metadata_and_installed_management() {
        const value = view();
        value.menu.snapshot = Object.assign({}, value.menu.snapshot, {
            packages: [
                {
                    name: "obs-studio",
                    version: "32.0.0"
                }
            ],
            capabilities: {
                packages: true,
                aur: true
            }
        });
        const row = value.menu.softwareRows("software.discover").find(row => row.package === "obs-studio");
        verify(row.checked);
        compare(row.operation, "info");
        verify(row.website.startsWith("https://"));
        const actions = value.menu.itemActions(row);
        verify(actions.some(row => row.operation === "remove-package"));
        verify(actions.some(row => row.operation === "tool" && row.value === "updates"));
        const chrome = value.menu.apps.find(row => row.name === "Google Chrome");
        verify(chrome.software);
        compare(chrome.package, "google-chrome");
        compare(chrome.kind, "app");
    }
    function test_bundle_requires_preview_and_routes_explicit_scope() {
        const value = view();
        value.menu.library.data = {
            themes: BundleData.rows
        };
        value.navigate("software.themes");
        const bundle = value.entries.find(row => row.bundle);
        verify(bundle);
        value.run(value.menu.itemActions(bundle).find(row => row.operation === "software-preview"));
        verify(value.softwareDetails.bundle);
        const worker = findChild(value.menu.library, "softwareLibraryAction");
        verify(!worker.running);
        const loader = findChild(value, "softwarePreviewLoader");
        tryVerify(() => !!loader.item);
        loader.item.bundleApplyRequested("light", "wallpaper");
        compare(worker.operation, "bundle-apply");
        compare(worker.payload.mode, "light");
        compare(worker.payload.scope, "wallpaper");
        worker.running = false;
        value.navigate("software.development");
        findChild(value, "launcherSearch").text = "catppuccin";
        verify(value.entries.some(row => row.bundle));
    }

    function test_themes_preview_light_and_dark_without_applying_until_requested() {
        const value = view();
        value.menu.library.data = {
            extensions: [],
            themes: [
                {
                    id: "sample",
                    name: "Sample theme",
                    group: "soft",
                    modes: {
                        dark: {
                            surface: "121212",
                            background: "101010",
                            primary: "ccccaa",
                            secondary: "bbccaa",
                            tertiary: "aabbcc",
                            onSurface: "ffffff"
                        },
                        light: {
                            surface: "eeeeee",
                            background: "ffffff",
                            primary: "333355",
                            onSurface: "111111"
                        }
                    }
                }
            ]
        };
        value.navigate("software.themes");
        const row = value.entries.find(row => row.theme);
        verify(row !== undefined);
        const oldCount = NacreWallpapers.preferences.length;
        value.softwareDetails = row;
        tryVerify(() => findChild(value, "softwarePreviewLoader").item !== null);
        const panel = findChild(value, "softwarePreviewLoader").item;
        panel.mode = "light";
        compare(NacreWallpapers.preferences.length, oldCount);
        panel.applyRequested("light");
        compare(NacreWallpapers.preferences.length, oldCount + 1);
        compare(NacreWallpapers.preferences[oldCount].palettePreset, "sample");
        compare(NacreWallpapers.preferences[oldCount].paletteMode, "light");
        verify(value.softwareDetails === null);
    }
    function test_extension_management_and_actions_use_the_native_worker() {
        const value = view();
        value.menu.library.data = {
            themes: [],
            extensions: [
                {
                    id: "test.shortcuts",
                    name: "Test shortcuts",
                    description: "Example",
                    version: "1.0",
                    source: "https://example.org/repo",
                    kinds: ["launcher"],
                    enabled: false,
                    actions: [
                        {
                            id: "notes",
                            name: "Open sample notes",
                            command: ["xdg-open", "/fixture/notes"]
                        }
                    ]
                }
            ]
        };
        value.navigate("software.extensions");
        const row = value.entries.find(row => row.extension);
        verify(row !== undefined);
        compare(row.status, "Disabled");
        verify(!value.menu.extensionActions().length);
        const worker = findChild(value.menu.library, "softwareLibraryAction");
        worker.running = false;
        value.run(row);
        compare(worker.command.slice(-3).join("|"), "software-library|enable|test.shortcuts");
        worker.running = false;
        value.menu.library.data = {
            themes: [],
            extensions: [Object.assign({}, row.extension, {
                    enabled: true
                })]
        };
        const action = value.menu.extensionActions()[0];
        compare(action.name, "Open sample notes");
        value.run(action);
        compare(worker.command.slice(-3).join("|"), "software-library|open|test.shortcuts");
        compare(worker.payload.action, "notes");
        worker.running = false;
        value.run(value.menu.node("software.extensions.import"));
        compare(value.form.software, "import");
        value.submitForm({
            url: "https://example.org/extension.git"
        });
        compare(worker.command.slice(-3).join("|"), "software-library|import|");
        compare(worker.payload.url, "https://example.org/extension.git");
        verify(value.formPending);
        worker.running = false;
        value.cancelForm();
    }

    function test_speed_tests_are_explicit_confirmed_and_searchable() {
        const value = view();
        value.menu.library.data = Object.assign({}, value.menu.library.data, {
            system: {
                speed: [
                    {
                        id: "disk-speed",
                        name: "Disk speed test",
                        description: "Owned temporary file",
                        section: "health.speed",
                        root: false
                    }
                ]
            }
        });
        value.navigate("health.speed");
        compare(value.entries.length, 1);
        verify(!!value.entries[0].confirm);
        verify(!value.menu.execute(value.entries[0]));
        value.navigate("software.development");
        findChild(value, "launcherSearch").text = "disk speed";
        verify(value.entries.some(row => row.value === "disk-speed"));
    }

    function test_hardware_actions_are_filtered_and_globally_searchable() {
        const value = view();
        value.menu.library.data = Object.assign({}, value.menu.library.data, {
            system: {
                hardware: [
                    {
                        id: "touchpad-disable",
                        name: "Disable touchpad",
                        description: "Detected supported touchpad",
                        section: "setup.hardware",
                        root: false
                    },
                    {
                        id: "laptop-display-off",
                        name: "Turn off laptop display",
                        description: "External output is active",
                        section: "setup.hardware",
                        root: false
                    }
                ]
            }
        });
        value.navigate("setup.hardware");
        compare(value.entries.length, 2);
        verify(!value.entries.some(row => row.value === "hibernate" || row.value === "laptop-mirror"));
        const entry = value.entries.find(row => row.value === "touchpad-disable");
        verify(!!entry.confirm);
        value.navigate("software.development");
        findChild(value, "launcherSearch").text = "touchpad";
        verify(value.entries.some(row => row.value === "touchpad-disable"));
    }

    function test_bring_visitors_is_capability_gated_and_dispatches_the_scoped_helper() {
        const value = view();
        let action = value.menu.node("actions.bring-visitors");
        verify(action.disabled);
        verify(!value.menu.execute(action));
        compare(AppLaunch.commands.length, 0);
        NacreCompositor.backendName = "niri";
        action = value.menu.node("actions.bring-visitors");
        verify(!action.disabled);
        verify(value.menu.execute(action));
        compare(AppLaunch.commands.length, 1);
        compare(AppLaunch.commands[0].slice(-2).join(","), "monitor-workspaces,bring");
    }

    function test_setup_actions_use_hardware_results_and_confirmation() {
        const value = view();
        value.menu.library.data = Object.assign({}, value.menu.library.data, {
            system: {
                setup: [
                    {
                        id: "dns-cloudflare",
                        name: "Cloudflare DNS",
                        description: "Resolver addresses",
                        section: "setup.system",
                        root: true
                    },
                    {
                        id: "fingerprint-enroll",
                        name: "Enroll a fingerprint",
                        description: "Detected reader",
                        section: "setup.system",
                        root: false
                    }
                ]
            }
        });
        value.navigate("setup.system");
        compare(value.entries.length, 2);
        const dns = value.entries.find(row => row.value === "dns-cloudflare");
        verify(!!dns.confirm);
        verify(!value.menu.execute(dns));
        value.menu.execute(dns, true);
        const worker = findChild(value.menu.library, "softwareLibraryAction");
        compare(worker.operation, "system-open");
        compare(worker.value, "dns-cloudflare");
        worker.running = false;
        value.navigate("software.development");
        findChild(value, "launcherSearch").text = "fingerprint";
        verify(value.entries.some(row => row.value === "fingerprint-enroll"));
        value.menu.library.data = Object.assign({}, value.menu.library.data, {
            system: {
                setup: []
            }
        });
        value.navigate("setup.system");
        compare(value.entries.length, 0);
    }

    function test_health_actions_are_searchable_and_require_confirmation() {
        const value = view();
        value.menu.library.data = Object.assign({}, value.menu.library.data, {
            system: {
                health: [
                    {
                        id: "audio-restart",
                        name: "Restart audio",
                        description: "Audio pauses",
                        section: "health.fixits",
                        root: false
                    },
                    {
                        id: "cache-clean",
                        name: "Clean package caches",
                        description: "Retain three versions",
                        section: "health.fixits",
                        root: true
                    }
                ]
            }
        });
        value.navigate("health.fixits");
        compare(value.entries.length, 2);
        const entry = value.entries.find(row => row.value === "cache-clean");
        verify(!!entry.confirm);
        verify(!value.menu.execute(entry));
        value.menu.execute(entry, true);
        const worker = findChild(value.menu.library, "softwareLibraryAction");
        compare(worker.operation, "system-open");
        compare(worker.value, "cache-clean");
        worker.running = false;
        value.navigate("software.development");
        findChild(value, "launcherSearch").text = "restart audio";
        verify(value.entries.some(row => row.value === "audio-restart"));
        value.menu.library.data = Object.assign({}, value.menu.library.data, {
            system: {
                health: []
            }
        });
        value.navigate("health.fixits");
        compare(value.entries.length, 0);
    }

    function test_curated_fonts_install_or_select_discovered_family_without_duplicates() {
        const value = view();
        value.menu.snapshot = Object.assign({}, value.menu.snapshot, {
            fonts: ["JetBrains Mono", "FiraCode Nerd Font", "Other Font"],
            capabilities: {
                packages: true,
                aur: false
            }
        });
        value.navigate("style.fonts");
        const jetbrains = value.entries.find(row => row.name === "JetBrains Mono");
        compare(jetbrains.operation, "font");
        compare(jetbrains.value, "JetBrains Mono");
        compare(value.entries.filter(row => row.value === "JetBrains Mono").length, 1);
        const fira = value.entries.find(row => row.name === "Fira Code Nerd");
        compare(fira.value, "FiraCode Nerd Font");
        const missing = value.entries.find(row => row.name === "Cascadia Code");
        compare(missing.operation, "install-package");
        verify(!!missing.confirm);
        const aur = value.entries.find(row => row.name === "Victor Mono");
        verify(aur.disabled);
        value.menu.execute(jetbrains);
        compare(DesktopSettings.data.interfaceFont, "JetBrains Mono");
        value.navigate("software.development");
        findChild(value, "launcherSearch").text = "meslo";
        verify(value.entries.some(row => row.name === "Meslo Nerd"));
    }

    function test_aur_policy_is_searchable_from_other_sections() {
        const value = view();
        value.navigate("apps");
        findChild(value, "launcherSearch").text = "aur review";
        const row = value.entries.find(row => row.id === "software.packages.review-policy");
        verify(row);
        value.run(row);
        compare(NacrePanelState.calls[NacrePanelState.calls.length - 1], ["settings", "system"]);
    }

    function test_monospace_font_is_globally_searchable_and_independent() {
        const value = view();
        value.menu.snapshot = Object.assign({}, value.menu.snapshot, {
            fonts: ["JetBrains Mono", "FiraCode Nerd Font"],
            capabilities: {
                packages: true,
                aur: false
            }
        });
        DesktopSettings.set("interfaceFont", "IBM Plex Sans");
        value.navigate("style.mono-fonts");
        const row = value.entries.find(row => row.value === "FiraCode Nerd Font");
        compare(row.operation, "mono-font");
        value.menu.execute(row);
        compare(DesktopSettings.data.monoFont, "FiraCode Nerd Font");
        compare(DesktopSettings.data.interfaceFont, "IBM Plex Sans");
        value.navigate("software.development");
        findChild(value, "launcherSearch").text = "terminal font fira";
        verify(value.entries.some(row => row.operation === "mono-font"));
        value.navigate("style.mono-fonts");
        value.menu.execute(value.entries.find(row => row.value === ""));
        compare(DesktopSettings.data.monoFont, "");
    }

    function test_tui_creation_literal_arguments_search_and_removal() {
        const value = view();
        value.menu.library.data = Object.assign({}, value.menu.library.data, {
            tui: {
                terminals: ["kitty"],
                rows: [
                    {
                        id: "0123456789abcdef0123456789abcdef",
                        name: "My monitor",
                        command: ["htop"],
                        terminal: "kitty",
                        icon: "utilities-terminal",
                        available: true
                    }
                ]
            }
        });
        value.navigate("software.tui");
        const entry = value.entries.find(row => row.tui);
        verify(entry);
        verify(value.menu.itemActions(entry).some(action => action.operation === "tui-remove" && !!action.confirm));
        value.run(value.menu.node("software.tui.new"));
        compare(value.form.kind, "tui");
        value.submitForm({
            name: "Literal app",
            command: "printf\nvalue; $(not executed)",
            terminal: "kitty",
            icon: "utilities-terminal"
        });
        const worker = findChild(value.menu.library, "softwareLibraryAction");
        compare(worker.operation, "tui-create");
        compare(worker.payload.command[0], "printf");
        compare(worker.payload.command[1], "value; $(not executed)");
        verify(!AppLaunch.commands.some(command => command[0] === "printf"));
        worker.running = false;
        value.menu.library.completed("tui-create", true);
        value.navigate("software.development");
        findChild(value, "launcherSearch").text = "my monitor";
        verify(value.entries.some(row => row.tui));
    }

    function test_ai_category_packages_and_web_apps_keep_global_search() {
        const value = view();
        value.menu.library.data = Object.assign({}, value.menu.library.data, {
            ai: {
                chatgptPackage: false,
                webApps: true,
                terminal: true
            }
        });
        value.menu.snapshot = Object.assign({}, value.menu.snapshot, {
            capabilities: {
                packages: true,
                aur: true
            },
            packages: [],
            webapps: []
        });
        value.navigate("software.discover");
        value.softwareCategory = "ai";
        compare(value.entries.length, 4);
        verify(value.entries.every(row => row.category === "ai"));
        const chatgpt = value.entries.find(row => row.name === "ChatGPT");
        compare(chatgpt.operation, "webapp-offer");
        verify(!chatgpt.disabled);
        const lm = value.entries.find(row => row.name === "LM Studio");
        compare(lm.value, "lmstudio-bin");
        value.menu.library.data = Object.assign({}, value.menu.library.data, {
            ai: {
                chatgptPackage: true,
                webApps: true,
                terminal: true
            }
        });
        compare(value.entries.find(row => row.name === "ChatGPT").operation, "install-package");
        value.menu.snapshot = Object.assign({}, value.menu.snapshot, {
            packages: [
                {
                    name: "ollama-cuda"
                }
            ],
            webapps: [
                {
                    id: "sample",
                    label: "ChatGPT",
                    url: "https://chatgpt.com/"
                }
            ]
        });
        const ollama = value.entries.find(row => row.name === "Ollama");
        compare(ollama.operation, "ai-terminal");
        compare(ollama.package, "ollama-cuda");
        compare(value.entries.find(row => row.name === "ChatGPT").operation, "webapp");
        value.navigate("software.development");
        findChild(value, "launcherSearch").text = "chatgpt";
        verify(value.entries.some(row => row.name === "ChatGPT"));
    }

    function test_development_actions_are_searchable_gated_and_terminal_visible() {
        const value = view();
        value.menu.library.data = Object.assign({}, value.menu.library.data, {
            environments: {
                profiles: [
                    {
                        id: "node",
                        name: "Node.js",
                        kind: "runtime",
                        available: true,
                        installed: true
                    },
                    {
                        id: "postgres",
                        name: "PostgreSQL",
                        kind: "database",
                        available: false,
                        installed: false,
                        reason: "Docker is unavailable"
                    }
                ]
            }
        });
        value.navigate("software.development");
        compare(value.entries.filter(row => row.operation === "environment").length, 4);
        const remove = value.entries.find(row => row.value === "node" && row.environmentOperation === "remove");
        verify(!remove.disabled);
        verify(!!remove.confirm);
        verify(value.entries.filter(row => row.value === "postgres").every(row => row.disabled));
        const field = findChild(value, "launcherSearch");
        field.text = "node";
        verify(value.entries.some(row => row.operation === "environment"));
        verify(!value.menu.execute(remove));
        value.menu.execute(remove, true);
        verify(AppLaunch.commands.some(call => call[0] === "kitty" && call.includes("software-environments") && call.includes("remove")));
    }

    function test_owner_source_switches_and_web_engine_are_live_reader_values() {
        const value = view(), field = findChild(value, "launcherSearch");
        value.menu.localSourceOptions = {
            version: 1,
            brain: false,
            files: false,
            calculator: false,
            actions: false,
            ai: false,
            webEngine: "brave"
        };
        value.menu.fileQuery = "report";
        value.menu.fileRows = [
            {
                id: "doc",
                name: "Report.md",
                path: "/fixture/report.md"
            }
        ];
        value.menu.brainQuery = "report";
        value.menu.notes = [
            {
                path: "report.md",
                title: "Report note",
                preview: "Report"
            }
        ];
        field.text = "report";
        verify(!value.entries.some(row => row.file || row.note));
        value.menu.prepare("home", "report");
        compare(value.menu.fileWanted, "");
        compare(value.menu.brainWanted, "");
        field.text = "= 7+5";
        verify(!value.calculation);
        verify(value.entries[0].disabled);
        field.text = "> screenshot";
        verify(value.entries[0].disabled);
        field.text = "? hello";
        verify(value.entries[0].disabled);
        AppLaunch.commands = [];
        value.menu.execute({
            operation: "search-web",
            value: "literal space & query"
        });
        verify(AppLaunch.commands[0][2].startsWith("https://search.brave.com/search?q="));
        verify(AppLaunch.commands[0][2].endsWith("literal%20space%20%26%20query"));
        value.menu.localSourceOptions = {
            version: 1,
            brain: true,
            files: true,
            calculator: true,
            actions: true,
            ai: true,
            webEngine: "duckduckgo"
        };
        field.text = "= 7+5";
        verify(value.calculation);
        compare(value.entries[0].value, "12");
    }
    function test_filter_applies_before_global_result_limit() {
        const value = view(), field = findChild(value, "launcherSearch");
        NacreCompositor.windows = Array.from({
            length: 130
        }, (_, index) => ({
                    windowId: String(index),
                    title: "Report window " + index,
                    wmClass: "editor"
                }));
        NacreMenu.fileQuery = "report";
        NacreMenu.fileRows = [
            {
                id: "doc",
                name: "Design report.md",
                path: "/fixture/report.md"
            }
        ];
        field.text = "report";
        verify(!value.entries.some(row => row.file));
        value.resultFilter = "files";
        compare(value.entries.length, 1);
        verify(value.entries[0].file);
        compare(value.captureState().resultFilter, "files");
    }
    function test_screenshot_uses_saved_capture_follow_up_after_closing() {
        const value = view();
        const worker = findChild(NacreMenu.workflows, "launcherWorkflowAction");
        worker.running = false;
        value.run(NacreMenu.node("actions.capture.screen"));
        verify(!value.visibilities.launcher);
        compare(AppLaunch.commands.length, 0);
        tryVerify(() => worker.running);
        compare(worker.command.slice(-3).join("|"), "launcher-workflows|capture|capture-screen");
        worker.running = false;
    }
    function test_missing_common_apps_offer_install_from_ordinary_search() {
        const value = view(), field = findChild(value, "launcherSearch");
        value.menu.snapshot = Object.assign({}, value.menu.snapshot, {
            capabilities: {
                packages: true,
                aur: true
            },
            packages: []
        });
        const originalApps = NacreApps.all;
        NacreApps.all = [];
        for (const query of ["discord", "chrome", "steam", "obs"]) {
            field.text = query;
            const offer = value.entries.find(row => row.operation === "install-package" && row.installOffer);
            verify(offer !== undefined, query);
            verify(!offer.disabled);
            verify(offer.description.startsWith("Install "));
        }
        field.text = "disc";
        compare(value.entries[0].package, "discord");
        const entry = value.entries[0];
        value.run(entry);
        verify(AppLaunch.commands.length > 0);
        compare(AppLaunch.commands[0].slice(-2).join("|"), "install|discord");
        NacreApps.all = originalApps;
        const oldApps = NacreApps.all;
        try {
            NacreApps.all = [...oldApps,
                {
                    id: "com.discordapp.Discord",
                    name: "Discord",
                    icon: "",
                    comment: "Chat"
                }
            ];
            field.text = "discord";
            verify(!value.entries.some(row => row.installOffer && row.package === "discord"));
            compare(value.entries[0].kind, "app");
        } finally {
            NacreApps.all = oldApps;
        }
        value.menu.snapshot = Object.assign({}, value.menu.snapshot, {
            packages: [
                {
                    name: "steam"
                }
            ],
            capabilities: {
                packages: true,
                aur: false
            }
        });
        field.text = "steam";
        verify(!value.entries.some(row => row.installOffer && row.package === "steam"));
        NacreApps.all = [];
        field.text = "chrome";
        const unsupported = value.entries.find(row => row.installOffer && row.package === "google-chrome");
        verify(unsupported.disabled);
        verify(unsupported.reason.includes("paru"));
        field.text = "spotify";
        verify(value.entries.find(row => row.installOffer && row.package === "spotify").disabled);
        NacreApps.all = originalApps;
    }

    function test_full_emoji_catalog_and_stop_are_available_during_other_work() {
        tryVerify(() => NacreMenu.emojiCatalog.length === 3944);
        const penguin = NacreMenu.emojiCatalog.find(row => row.value === "🐧");
        NacreMenu.prepare("home", "penguin");
        const search = findChild(NacreMenu.previews, "launcherEmojiSearch");
        verify(search !== null);
        tryVerify(() => search.running && search.query === "penguin");
        search.stdout.text = JSON.stringify({
            entries: [penguin]
        });
        search.stdout.streamFinished();
        search.finish();
        const emoji = NacreMenu.provider("actions.emoji", "penguin").find(row => row.name === "penguin");
        verify(emoji !== undefined);
        compare(emoji.emoji, "🐧");
        const worker = findChild(NacreMenu.workflows, "launcherWorkflowAction");
        worker.running = true;
        verify(NacreMenu.workflows.work("record-stop"));
        verify(findChild(NacreMenu.workflows, "launcherRecordingStop").running);
        worker.running = false;
        findChild(NacreMenu.workflows, "launcherRecordingStop").running = false;
    }
    function test_unchanged_workflow_status_does_not_rebuild_search_models() {
        const worker = findChild(NacreMenu.workflows, "launcherWorkflowLoad");
        verify(worker !== null);
        let changes = 0;
        const changed = () => changes++;
        NacreMenu.workflows.dataChanged.connect(changed);
        try {
            worker.stdout.text = JSON.stringify(NacreMenu.workflows.data);
            worker.stdout.streamFinished();
            compare(changes, 0);
            const update = Object.assign({}, NacreMenu.workflows.data, {
                ready: true
            });
            worker.stdout.text = JSON.stringify(update);
            worker.stdout.streamFinished();
            compare(changes, 1);
            worker.stdout.streamFinished();
            compare(changes, 1);
        } finally {
            NacreMenu.workflows.dataChanged.disconnect(changed);
        }
    }

    function test_emoji_search_keeps_apps_immediate_and_discards_stale_results() {
        const value = view(), field = findChild(value, "launcherSearch");
        const search = findChild(value.menu.previews, "launcherEmojiSearch");
        search.running = false;
        field.text = "editor";
        compare(value.entries[0].name, "Editor");
        value.menu.prepare(value.route, value.query);
        tryVerify(() => search.running && search.query === "editor");
        const pending = value.menu.previews;
        field.text = "penguin";
        value.menu.prepare(value.route, value.query);
        search.stdout.text = JSON.stringify({
            entries: [
                {
                    value: "😀",
                    name: "wrong old match",
                    aliases: []
                }
            ]
        });
        search.stdout.streamFinished();
        compare(pending.emojiMatches.length, 0);
        search.finish();
        tryVerify(() => search.running && search.query === "penguin");
        search.stdout.text = JSON.stringify({
            entries: [
                {
                    value: "🐧",
                    name: "penguin",
                    aliases: ["bird"]
                }
            ]
        });
        search.stdout.streamFinished();
        search.finish();
        verify(value.entries.some(row => row.emoji === "🐧"));
        field.text = "";
        value.menu.prepare(value.route, value.query);
        compare(pending.emojiWanted, "");
        search.stdout.streamFinished();
        compare(pending.emojiMatches.length, 0);
    }

    function test_toggle_keeps_english_geometry_and_expands_wrapped_translations() {
        const previousLanguage = NacreI18n.language;
        try {
            NacreI18n.language = "en";
            const value = view();
            const button = findChild(value, "launcherToggle_dnd");
            compare(button.height, 50);
            const caption = button.children.find(item => item.text === button.text);
            const description = button.children.find(item => item.text === button.description);
            verify(caption !== undefined);
            verify(description !== undefined);
            verify(Math.abs(caption.y + caption.height / 2 - 16) <= 0.5);
            compare(description.y, 29);
            NacreI18n.language = "pseudo";
            button.width = 150;
            tryVerify(() => button.height > 50);
            verify(caption.lineCount > 1);
            verify(caption.y + caption.height <= description.y);
            verify(description.y + description.height <= button.height - 6);
        } finally {
            NacreI18n.language = previousLanguage;
        }
    }

    function test_pseudo_keeps_media_capability_names_and_translates_disabled_reasons() {
        const previousLanguage = NacreI18n.language;
        try {
            NacreI18n.language = "pseudo";
            compare(NacreMenu.node("actions.media.next").disabled, false);
            const previousPlayer = NacrePlayers.active;
            NacrePlayers.active = null;
            const row = NacreMenu.node("actions.media.next");
            verify(row.disabled);
            verify(row.reason.startsWith("["));
            verify(!row.reason.includes("No active media player"));
            NacrePlayers.active = previousPlayer;
        } finally {
            NacreI18n.language = previousLanguage;
        }
    }

    function test_shell_surface_heading_and_favorites_layout() {
        LauncherPreferences.favorites = ["editor"];
        const value = view();
        compare(value.border.width, 0);
        compare(value.color.toString(), "#00000000");
        const heart = findChild(value, "launcherFavorite_0");
        verify(!heart.visible);
        value.navigate("apps");
        const back = findChild(value, "launcherBack");
        compare(back.width, back.height);
        const heading = findChild(value, "launcherHeading");
        compare(Math.round(back.y + back.height / 2), Math.round(heading.y + heading.height / 2));
    }

    function test_files_reject_stale_results_and_conversion_uses_owner() {
        const value = view(), field = findChild(value, "launcherSearch");
        NacreMenu.fileRows = [
            {
                "id": "one",
                "name": "Report.pdf",
                "path": "/fixture/Report.pdf"
            }
        ];
        NacreMenu.fileQuery = "report";
        field.text = ": report";
        wait(20);
        compare(value.entries[0].name, "Report.pdf");
        field.text = ": other";
        wait(20);
        compare(value.entries.length, 0);
        NacreMenu.conversionQuery = "10 km to miles";
        NacreMenu.conversion = {
            "ok": true,
            "value": "6.2137 mi"
        };
        field.text = "= 10 km to miles";
        wait(20);
        compare(value.entries[0].value, "6.2137 mi");
    }

    function test_hover_heart_and_action_anchor_stay_inside_drawer() {
        LauncherPreferences.favorites = ["editor"];
        const value = view();
        const tile = findChild(value, "launcherTile_0"), heart = findChild(value, "launcherFavorite_0");
        mouseMove(value, 5, 5);
        wait(20);
        verify(!heart.visible);
        mouseMove(tile, tile.width / 2, tile.height / 2);
        tryCompare(heart, "visible", true);
        mouseClick(heart, heart.width / 2, heart.height / 2);
        tryCompare(value, "entries", []);
        value.navigate("apps");
        value.keyboardSelection = true;
        value.selection = 0;
        wait(20);
        const trigger = findChild(value, "launcherDetailActions");
        value.actionsFor(value.selectedEntry, trigger);
        const popup = findChild(value, "launcherItemActions");
        tryCompare(popup, "opened", true);
        wait(30);
        verify(popup.height > 50, "popup height " + popup.height + " content " + popup.contentItem.implicitHeight);
        verify(popup.x + popup.width <= value.width - 8);
        verify(popup.y + popup.height <= value.height - 86);
        verify(popup.x < value.contextAnchor.x);
        compare(Math.round(popup.y), Math.round(value.contextAnchor.y));
        value.height = 340;
        value.contextEntry = Object.assign({}, value.selectedEntry, {
            reason: "Additional details ".repeat(50),
            disabled: true
        });
        wait(30);
        verify(popup.y >= 8);
        verify(popup.y + popup.height <= value.height - 86);
    }
    function test_ethernet_only_launcher_radio_action_is_unavailable() {
        NacreNetwork.wifiAvailable = false;
        const row = NacreMenu.node("actions.controls.wifi");
        verify(row.disabled);
        compare(row.reason, "No Wi-Fi adapter");
        verify(!NacreMenu.execute(row));
        compare(AppLaunch.commands.length, 0);
        // A stale row must also be harmless after hardware disappears.
        row.disabled = false;
        NacreMenu.execute(row);
        compare(AppLaunch.commands.length, 0);
        NacreNetwork.wifiAvailable = true;
    }
    function test_controls_can_be_restored_and_show_authoritative_state() {
        const value = view();
        value.hoveredEntry = NacreMenu.apps[0];
        verify(!!value.previewEntry);
        value.showQuickControls();
        wait(30);
        compare(value.previewEntry, null);
        const dnd = findChild(value, "launcherToggle_dnd");
        verify(dnd.visible);
        verify(dnd.toggle);
        DesktopSettings.set("dnd", false);
        dnd.clicked();
        compare(dnd.selected, true);
        dnd.clicked();
        compare(dnd.selected, false);
        compare(value.previewEntry, null);
        const night = findChild(value, "launcherToggle_night-light");
        night.clicked();
        compare(night.selected, true);
        night.clicked();
        compare(night.selected, false);
        LauncherPreferences.recent = ["nacre-menu:actions.controls.night-light", "editor"];
        compare(NacreMenu.recent().length, 1);
        compare(NacreMenu.recent()[0].id, "editor");
    }
    function test_sections_and_five_row_apps() {
        const value = view();
        compare(NacreMenu.navigation.length, 9);
        value.navigate("apps");
        compare(value.entries.length, 4);
        const grid = findChild(value, "launcherApps");
        verify(grid.cellHeight >= 72);
        verify(grid.cellHeight <= 100);
        compare(value.selectedEntry, null);
        value.navigate("actions");
        verify(value.entries.some(row => {
            return row.id === "actions.capture";
        }));
        verify(!NacreMenu.navigation.some(row => {
            return ["games", "office", "development"].includes(row.id);
        }));
    }

    function test_maximized_grid_keeps_tile_spacing_bounded() {
        const value = view();
        value.width = 1920;
        value.height = 1100;
        value.navigate("apps");
        const grid = findChild(value, "launcherApps");
        verify(grid.height > 800);
        compare(grid.cellHeight, 100);
        verify(grid.cellWidth >= 110);
        verify(grid.cellWidth < 140);
    }
    function test_query_selects_and_launches_real_app_owner() {
        const value = view(), field = findChild(value, "launcherSearch");
        field.text = "google";
        wait(20);
        compare(value.selectedEntry.name, "Google Chrome");
        value.run(value.selectedEntry);
        compare(NacreApps.launched, "google-chrome");
        compare(value.visibilities.launcher, false);
    }

    function test_no_idle_selection_and_alphabetical_mixed_favorites() {
        LauncherPreferences.favorites = ["music", "editor", "nacre-menu:actions.terminal"];
        const value = view();
        compare(value.entries.map(row => {
            return row.name;
        }).join(","), "Editor,Music,Terminal");
        compare(value.selection, -1);
        value.activate(0);
        compare(NacreApps.launched, "");
        value.focusResults();
        compare(value.selection, 0);
        value.favorite(value.entries[0]);
        verify(!LauncherPreferences.favorites.includes("editor"));
    }

    function test_deep_navigation_search_scope_and_back() {
        const value = view(), field = findChild(value, "launcherSearch");
        value.navigate("style.personality");
        verify(value.entries.some(row => {
            return row.value === "pearl";
        }));
        value.run(value.entries.find(row => {
            return row.value === "pearl";
        }));
        compare(NacreWallpapers.appearancePreferences.palettePersonality, "pearl");
        value.back();
        compare(value.route, "home");
        field.text = "> wifi";
        wait(20);
        compare(value.selectedEntry.id, "setup.network");
        value.run(value.selectedEntry);
        compare(NacrePanelState.calls[0][1], "network");
    }

    function test_calculator_and_explicit_ai_guard() {
        const value = view(), field = findChild(value, "launcherSearch");
        field.text = "= (12+8)/5";
        wait(20);
        compare(value.selectedEntry.value, "4");
        value.run(value.selectedEntry);
        compare(AppLaunch.commands[0][0], "wl-copy");
        value.visibilities.launcher = true;
        field.text = "? Explain shells";
        wait(20);
        verify(!value.selectedEntry.disabled);
        SidebarChat.busy = true;
        wait(20);
        verify(value.entries[0].disabled);
        value.run(value.entries[0]);
        compare(SidebarChat.commands.length, 0);
    }
    function test_record_screen_routes_after_closing_and_obeys_recording_state() {
        const value = view();
        value.menu.snapshot = Object.assign({}, value.menu.snapshot, {
            capabilities: Object.assign({}, value.menu.snapshot.capabilities, {
                record: true
            }),
            recording: false
        });
        value.navigate("actions.capture");
        const screen = value.entries.find(row => row.value === "record-screen");
        verify(screen !== undefined);
        verify(!screen.disabled);
        compare(screen.name, "Record screen");
        value.run(screen);
        verify(value.visibilities.launcher);
        compare(value.form.kind, "recording");
        compare(AppLaunch.commands.length, 0);
        const sound = findChild(value, "menuFormChoice_system");
        verify(sound !== null);
        wait(40);
        mouseClick(sound, sound.width / 2, sound.height / 2);
        verify(sound.selected);
        const submit = findChild(value, "menuFormSubmit");
        mouseClick(submit, submit.width / 2, submit.height / 2);
        compare(value.visibilities.launcher, false);
        compare(value.menu.workflows.recordingOptions.audio, "system");
        verify(value.menu.workflows.recordingOptions.screen);
        compare(AppLaunch.commands.length, 0);
        const action = findChild(value.menu.workflows, "launcherWorkflowAction");
        tryVerify(() => action.running);
        compare(action.command.slice(-3).join("|"), "launcher-workflows|record|screen");
        value.menu.snapshot = Object.assign({}, value.menu.snapshot, {
            recording: true
        });
        verify(value.menu.node("actions.capture.record-screen").disabled);
        verify(value.menu.node("actions.capture.record").disabled);
        verify(!value.menu.node("actions.capture.stop").disabled);
        value.menu.snapshot = Object.assign({}, value.menu.snapshot, {
            recording: false,
            capabilities: Object.assign({}, value.menu.snapshot.capabilities, {
                record: false
            })
        });
        verify(value.menu.node("actions.capture.record-screen").disabled);
    }

    function calculatorClick(value, name) {
        const button = findChild(value, "calculatorKey_" + name);
        verify(button !== null, "Missing calculator key " + name);
        mouseClick(button, button.width / 2, button.height / 2);
    }

    function test_calculator_keypad_mouse_editing_and_chaining() {
        const value = view(), field = findChild(value, "launcherSearch");
        compare(findChild(value, "launcherCalculator"), null);
        field.text = "= ";
        tryVerify(() => findChild(value, "calculatorKey_7") !== null);
        compare(findChild(value, "calculatorResult").text, "0");
        compare(findChild(value, "calculatorCopy").enabled, false);
        for (const key of ["left-parenthesis", "1", "2", "add", "8", "right-parenthesis", "divide", "5"])
            calculatorClick(value, key);
        compare(value.scope.text, "(12+8)/5");
        compare(value.calculationResult.value, "4");
        calculatorClick(value, "equals");
        compare(value.scope.text, "4");
        verify(value.visibilities.launcher);
        calculatorClick(value, "multiply");
        calculatorClick(value, "3");
        calculatorClick(value, "equals");
        compare(value.scope.text, "12");
        const state = value.captureState();
        value.navigate("apps");
        value.restoreState(state);
        verify(value.calculatorAnswerReady);
        // A digit after equals starts a fresh calculation.
        calculatorClick(value, "7");
        compare(value.scope.text, "7");
        calculatorClick(value, "add");
        calculatorClick(value, "multiply");
        compare(value.scope.text, "7*");
        calculatorClick(value, "backspace");
        calculatorClick(value, "decimal");
        calculatorClick(value, "5");
        compare(value.calculationResult.value, "7.5");
        calculatorClick(value, "sign");
        compare(value.calculationResult.value, "-7.5");
        calculatorClick(value, "clear");
        compare(value.scope.kind, "calculator");
        compare(value.scope.text, "");
        calculatorClick(value, "1");
        calculatorClick(value, "2");
        calculatorClick(value, "backspace");
        compare(value.scope.text, "1");
        calculatorClick(value, "backspace");
        calculatorClick(value, "backspace");
        compare(value.scope.kind, "calculator");
        compare(value.scope.text, "");
        // Button input respects the typed field's cursor and selection.
        field.text = "= 123";
        field.select(3, 4);
        calculatorClick(value, "9");
        compare(value.scope.text, "193");
        field.cursorPosition = 3;
        calculatorClick(value, "decimal");
        compare(value.scope.text, "1.93");
        field.cursorPosition = 5;
        calculatorClick(value, "backspace");
        compare(value.scope.text, "1.3");
        compare(field.cursorPosition, 4);
        field.text = "= 16";
        calculatorClick(value, "sqrt");
        compare(value.calculationResult.value, "4");
        calculatorClick(value, "square");
        compare(value.calculationResult.value, "16");
        value.navigate("apps");
        tryCompare(findChild(value, "launcherCalculatorLoader"), "active", false);
    }

    function test_calculator_copy_errors_and_keyboard_focus() {
        const value = view(), field = findChild(value, "launcherSearch");
        field.text = "= 1/0";
        tryVerify(() => findChild(value, "calculatorKey_equals") !== null);
        compare(findChild(value, "calculatorKey_equals").enabled, false);
        compare(findChild(value, "calculatorCopy").enabled, false);
        verify(findChild(value, "calculatorError").visible);
        const pad = findChild(value, "launcherCalculator");
        const position = findChild(value, "calculatorKey_7").mapToItem(pad, 0, 0).y;
        field.text = "= 8*3";
        compare(findChild(value, "calculatorKey_7").mapToItem(pad, 0, 0).y, position);
        const copy = findChild(value, "calculatorCopy");
        mouseClick(copy, copy.width / 2, copy.height / 2);
        compare(AppLaunch.commands[0][0], "wl-copy");
        compare(AppLaunch.commands[0][2], "24");
        verify(value.visibilities.launcher);
        compare(copy.text, "Copied");
        field.forceActiveFocus();
        keyClick(Qt.Key_Down);
        const seven = findChild(value, "calculatorKey_7");
        verify(seven.children.some(child => child.activeFocus));
        keyClick(Qt.Key_Space);
        // Down then Space activates a real, focused keypad button.
        compare(value.scope.text, "8*37");
        compare(copy.text, "Copy");
        field.text = "= 10 km to miles";
        NacreMenu.conversionQuery = "10 km to miles";
        NacreMenu.conversion = {
            ok: true,
            value: "6.2137 mi"
        };
        compare(value.calculationResult.value, "6.2137 mi");
        calculatorClick(value, "equals");
        compare(value.scope.text, "10 km to miles");
    }

    function test_calculator_small_window_scroll_and_pseudo_labels() {
        const value = view(), field = findChild(value, "launcherSearch");
        const previousLanguage = NacreI18n.language;
        try {
            NacreI18n.language = "pseudo";
            value.width = 540;
            value.height = 340;
            field.text = "= 2+2";
            tryVerify(() => findChild(value, "launcherCalculator") !== null);
            const pad = findChild(value, "launcherCalculator");
            const seven = findChild(value, "calculatorKey_7");
            tryVerify(() => seven.width >= 42 && seven.height >= 42);
            verify(pad.contentHeight > pad.height);
            pad.contentY = pad.contentHeight - pad.height;
            wait(20);
            calculatorClick(value, "equals");
            compare(value.scope.kind, "calculator");
            compare(value.scope.text, "4");
            verify(findChild(value, "calculatorCopy").text.startsWith("["));
        } finally {
            NacreI18n.language = previousLanguage;
        }
    }

    function test_power_requires_confirmation_and_route_never_executes() {
        const value = view();
        value.navigate("power.poweroff");
        compare(AppLaunch.commands.length, 0);
        compare(value.route, "power");
        const shutdown = value.entries.find(row => {
            return row.value === "poweroff";
        });
        value.run(shutdown);
        verify(!!value.confirmation);
        compare(AppLaunch.commands.length, 0);
        value.dismiss();
        compare(value.confirmation, null);
        compare(value.visibilities.launcher, true);
    }

    function test_hidden_restore_and_live_toggle() {
        const value = view();
        LauncherPreferences.hidden = ["music"];
        value.navigate("hidden");
        compare(value.entries.length, 1);
        value.run(value.entries[0]);
        compare(LauncherPreferences.hidden.length, 0);
        value.navigate("actions.controls");
        const dnd = value.entries.find(row => {
            return row.value === "dnd";
        });
        value.run(dnd);
        verify(DesktopSettings.data.dnd);
        compare(value.visibilities.launcher, true);
    }

    function test_global_search_includes_font_and_wallpaper_providers() {
        const value = view(), field = findChild(value, "launcherSearch");
        value.navigate("style.fonts");
        field.text = "Plex";
        wait(20);
        verify(value.entries.some(row => {
            return row.value === "IBM Plex Sans";
        }));
        value.navigate("style.wallpapers");
        field.text = "garden";
        wait(20);
        verify(value.entries.some(row => row.name === "Blue garden" && row.operation === "wallpaper"));
    }

    function test_escape_without_mouse_and_small_layout() {
        const value = view();
        const field = findChild(value, "launcherSearch");
        field.forceActiveFocus();
        keyClick(Qt.Key_Escape);
        compare(value.visibilities.launcher, false);
        value.visibilities.launcher = true;
        value.width = 760;
        value.height = 480;
        value.navigate("apps");
        verify(!value.showDetails);
        const grid = findChild(value, "launcherApps");
        verify(grid.cellHeight >= 72);
        verify(grid.cellHeight <= 100);
        verify(grid.x + grid.width <= value.width);
    }

    function test_legacy_favorite_and_branded_icon_contract() {
        LauncherPreferences.favorites = ["nacre-action:terminal:"];
        const value = view();
        compare(value.entries[0].name, "Open terminal");
        value.navigate("apps");
        wait(30);
        const grid = findChild(value, "launcherApps");
        const aiIndex = value.entries.findIndex(entry => {
            return entry.id === "nacre-ai";
        });
        const editorIndex = value.entries.findIndex(entry => {
            return entry.id === "editor";
        });
        const ai = findChild(grid, "launcherTile_" + aiIndex), editor = findChild(grid, "launcherTile_" + editorIndex);
        verify(findChild(ai, "nacreAiAppLogo").visible);
        verify(findChild(ai, "nacreAiAppLogo").ai);
        verify(!findChild(editor, "nacreAiAppLogo").visible);
    }

    function test_ctrl_d_from_search_and_favorite_keeps_apps_selection() {
        const value = view(), field = findChild(value, "launcherSearch");
        field.text = "editor";
        wait(20);
        field.forceActiveFocus();
        keyClick(Qt.Key_D, Qt.ControlModifier);
        verify(LauncherPreferences.favorites.includes("editor"));
        value.navigate("apps");
        value.focusResults();
        const before = value.selectedEntry.id;
        value.favorite(value.selectedEntry);
        compare(value.selectedEntry.id, before);
        compare(value.route, "apps");
    }

    function test_blank_actions_and_unmatched_text_never_run_or_send() {
        const value = view(), field = findChild(value, "launcherSearch");
        field.text = ">";
        wait(20);
        value.activate(0);
        compare(AppLaunch.commands.length, 0);
        compare(SidebarChat.commands.length, 0);
        field.text = "nothing matches this query";
        wait(20);
        compare(value.entries.length, 1);
        compare(value.entries[0].operation, "search-web");
        compare(AppLaunch.commands.length, 0);
        value.activate(0);
        compare(SidebarChat.commands.length, 0);
        compare(AppLaunch.commands[0][0], "nacre-app");
        verify(AppLaunch.commands[0][2].endsWith("nothing%20matches%20this%20query"));
    }

    function test_arithmetic_grammar_stays_bounded() {
        const valid = [["2+3*4", "14"], ["(2+3)*4", "20"], ["2^3^2", "512"], ["sqrt(81)", "9"], ["-2^2", "-4"], ["abs(-3)", "3"], ["round(1.6)", "2"]];
        for (const pair of valid) {
            const result = Arithmetic.evaluate(pair[0]);
            verify(result.ok);
            compare(result.value, pair[1]);
        }
        for (const expression of ["1/0", "process.exit()", "Math.random()", "1+", "(".repeat(40) + "1" + ")".repeat(40)])
            verify(!Arithmetic.evaluate(expression).ok);
    }

    function test_notes_never_show_results_from_a_previous_query() {
        const value = view(), field = findChild(value, "launcherSearch");
        NacreMenu.notes = [
            {
                "path": "old.md",
                "title": "Old result",
                "preview": "Old"
            }
        ];
        NacreMenu.brainQuery = "old";
        field.text = "# current";
        wait(20);
        compare(value.entries.length, 0);
        NacreMenu.brainQuery = "current";
        NacreMenu.notes = [
            {
                "path": "new.md",
                "title": "Current result",
                "preview": "Current"
            }
        ];
        wait(20);
        compare(value.entries[0].name, "Current result");
    }

    function test_shared_pick_and_input_modes_return_values() {
        const value = view();
        NacreMenuPrompt.ownerView = value.visibilities;
        NacreMenuPrompt.request = {
            "id": "test",
            "kind": "pick",
            "title": "Pick one",
            "choices": [
                {
                    "label": "One",
                    "value": "one"
                },
                {
                    "label": "Two",
                    "value": "two"
                }
            ]
        };
        NacreMenuPrompt.active = true;
        value.reset();
        const field = findChild(value, "launcherSearch");
        field.text = "Two";
        wait(20);
        value.run(value.selectedEntry);
        compare(NacreMenuPrompt.result[0], "two");
        compare(NacreMenuPrompt.result[1], false);
        NacreMenuPrompt.ownerView = value.visibilities;
        NacreMenuPrompt.request = {
            "id": "test2",
            "kind": "input",
            "title": "Name",
            "initial": "Example"
        };
        NacreMenuPrompt.active = true;
        value.reset();
        wait(30);
        const input = findChild(value, "menuFormField_value");
        verify(!!input);
        compare(input.text, "Example");
        input.text = "Chosen";
        input.forceActiveFocus();
        keyClick(Qt.Key_Return);
        compare(NacreMenuPrompt.result[0], "Chosen");
        NacreMenuPrompt.ownerView = null;
        NacreMenuPrompt.active = false;
        NacreMenuPrompt.request = null;
    }

    function test_initial_cli_leaf_route_selects_parent_without_execution() {
        const value = view();
        value.visibilities.launcherRoute = "setup.displays";
        value.reset();
        compare(value.route, "setup");
        compare(value.selectedEntry.id, "setup.displays");
        compare(NacrePanelState.calls.length, 0);
        value.visibilities.launcherRoute = "power.poweroff";
        value.reset();
        compare(value.route, "power");
        compare(value.selectedEntry.id, "power.poweroff");
        compare(AppLaunch.commands.length, 0);
    }

    function test_favorite_indicator_is_visible_and_filled_in_search_results() {
        LauncherPreferences.favorites = ["editor"];
        const value = view();
        const field = findChild(value, "launcherSearch");
        field.text = "editor";
        tryVerify(() => findChild(value, "launcherRow_0") !== null);
        const favorite = findChild(findChild(value, "launcherRow_0"), "launcherFavorite_0");
        verify(favorite);
        compare(favorite.visible, true);
        const glyph = findChild(favorite, "launcherFavoriteGlyph_0");
        verify(glyph);
        compare(glyph.text, "favorite");
        compare(glyph.fill, 1);

        LauncherPreferences.favorites = [];
        compare(glyph.fill, 0);
    }

    name: "NacreUniversalMenu"
    width: 1200
    height: 850
    visible: true
    when: windowShown

    Component {
        id: scene

        NacreAppBrowser {
            width: 1100
            height: 590

            visibilities: QtObject {
                property bool launcher: true
                property string launcherMode: "apps"
                property string launcherRoute: "home"
                property string launcherQuery: ""
                property int launcherRequest: 0
            }
        }
    }
}
