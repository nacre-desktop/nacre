import QtQuick
import QtTest
import "fixtures"

TestCase {
    id: test
    name: "NacreAppDiscovery"
    when: windowShown
    Component {
        id: service
        NacreApps {}
    }
    function entry(id, name, extra) {
        return Object.assign({
            id: id,
            name: name,
            comment: "",
            genericName: "",
            keywords: [],
            command: ["fixture", id],
            noDisplay: false,
            runInTerminal: false,
            workingDirectory: ""
        }, extra || {});
    }
    function init() {
        DesktopEntries.applications = {
            values: []
        };
        LauncherPreferences.hidden = [];
        AppLaunch.calls = [];
    }
    function test_ranking_acronyms_unicode_multiword_and_stable_original_entries() {
        const editor = entry("code", "Visual Studio Code", {
            keywords: ["Programming", "IDE"]
        });
        const accent = entry("edit", "Éditeur");
        const browser = entry("chrome", "Chrome", {
            genericName: "Web Browser",
            comment: "Open the web"
        });
        DesktopEntries.applications = {
            values: [browser, editor, accent]
        };
        const apps = createTemporaryObject(service, test);
        compare(apps.fuzzyQuery("vsc")[0], editor);
        compare(apps.fuzzyQuery("visual ide")[0], editor);
        compare(apps.fuzzyQuery("editeur")[0], accent);
        compare(apps.fuzzyQuery("browser")[0], browser);
        compare(apps.fuzzyQuery("CHROME")[0], browser);
        compare(apps.fuzzyQuery("chrm")[0], browser);
        compare(apps.fuzzyQuery("no such application xyz").length, 0);
        compare(apps.fuzzyQuery("  ").length, 3);
        compare(AppLaunch.calls.length, 0);
    }
    function test_hidden_duplicate_terminal_cwd_and_removed_or_foreign_launch() {
        const terminal = entry("term", "Terminal tool", {
            runInTerminal: true,
            workingDirectory: "/fixture",
            command: ["tool", "argument with spaces", "--literal=$value"]
        });
        const hidden = entry("hidden", "Hidden");
        const duplicate = entry("term", "Duplicate");
        DesktopEntries.applications = {
            values: [terminal, hidden, duplicate, entry("menu-less", "No menu", {
                    noDisplay: true
                })]
        };
        LauncherPreferences.hidden = ["hidden"];
        const apps = createTemporaryObject(service, test);
        compare(apps.all.length, 2);
        compare(apps.list.length, 1);
        verify(!apps.launch(hidden));
        compare(AppLaunch.calls.length, 0);
        verify(apps.launch(terminal));
        compare(AppLaunch.calls[0].command.join("|"), "kitty|--|tool|argument with spaces|--literal=$value");
        compare(AppLaunch.calls[0].cwd, "/fixture");
        DesktopEntries.applications = {
            values: []
        };
        verify(!apps.launch(terminal));
        verify(!apps.launch(entry("foreign", "Foreign")));
        compare(AppLaunch.calls.length, 1);
        compare(LauncherPreferences.hidden[0], "hidden");
    }
}
