import QtQuick
import QtTest
import "fixtures"

TestCase {
    id: test
    name: "NacreWallpaperProvider"
    when: windowShown

    function cleanup() {
        NacrePresentation.active = ({});
        NacrePresentation.pending = ({});
        NacrePanelState.screens = ({});
    }
    function entry(path, dynamic) {
        return {
            path: path,
            name: path.split('/').pop(),
            poster: path + ".png",
            preview: path + ".jpg",
            thumbnail: path + "-small.jpg",
            dynamic: !!dynamic,
            animated: !!dynamic,
            appliedAtMs: Date.now()
        };
    }
    function setup() {
        const view = NacreWallpapers;
        const catalog = findChild(view, "wallpaperCatalog");
        const data = {
            entries: [entry("/fixture/one"), entry("/fixture/two", true)],
            preferences: {
                kind: "static",
                layout: "carousel"
            },
            palettes: [],
            media: {}
        };
        catalog.stdout.text = JSON.stringify(data);
        catalog.stdout.streamFinished();
        catalog.running = false;
        catalog.exited(0, 0);
        return view;
    }
    function finish(process, text, code) {
        if (typeof process.stdout.read === "function")
            process.stdout.read(text);
        else {
            process.stdout.text = text;
            process.stdout.streamFinished();
        }
        process.running = false;
        process.exited(code || 0, 0);
    }
    function test_stream_does_not_complete_the_transaction_before_the_result() {
        const view = setup(), commit = findChild(view, "wallpaperCommit");
        view.setWallpaper("/fixture/two");
        view.commitSelection();
        commit.stdout.read(JSON.stringify({
            presentation: {
                poster: "/fixture/two.png",
                publishedAtMs: 2000
            }
        }));
        compare(NacrePresentation.pending.poster, "/fixture/two.png");
        verify(!view.commitReceived);
        compare(view.selectedPath, "/fixture/two");
        finish(commit, JSON.stringify(entry("/fixture/two", true)));
        compare(view.media.path, "/fixture/two");
        verify(view.commitReceived);
    }
    function test_catalogue_validation_retention_and_search() {
        const view = setup();
        compare(view.list.length, 2);
        const previous = view.list;
        verify(!view.publishCatalog('{"entries":[]}'));
        compare(view.list, previous);
        verify(view.error.length > 0);
        compare(view.fuzzyQuery("two")[0].path, "/fixture/two");
        compare(view.fuzzyQuery("zzz").length, 0);
        const data = {
            entries: [entry("/fixture/one"), entry("/fixture/one")],
            preferences: {
                kind: "static",
                layout: "carousel"
            },
            palettes: [],
            media: {}
        };
        verify(view.publishCatalog(JSON.stringify(data)));
        compare(view.list.length, 1);
    }
    function test_browsing_applies_before_close_and_coalesces_latest() {
        const view = setup(), commit = findChild(view, "wallpaperCommit");
        NacrePanelState.screens = {
            test: {
                launcher: true,
                launcherMode: "wallpaper"
            }
        };
        view.browse("/fixture/one");
        view.browse("/fixture/two");
        tryCompare(commit, "running", true, 400);
        compare(commit.requestPath, "/fixture/two");
        compare(commit.starts, 1);
        verify(view.pickerOpen);
        view.browse("/fixture/three");
        view.browse("/fixture/one");
        wait(170);
        compare(commit.starts, 1);
        finish(commit, JSON.stringify(entry("/fixture/two", true)));
        tryCompare(commit, "requestPath", "/fixture/one", 400);
        finish(commit, JSON.stringify(entry("/fixture/one")));
        compare(view.actualCurrent, "/fixture/one");
        verify(view.pickerOpen);
        compare(commit.starts, 2);
        view.commitSelection();
        wait(170);
        compare(commit.starts, 2);
    }
    function test_latest_selection_queue_and_failed_retry() {
        const view = setup(), commit = findChild(view, "wallpaperCommit");
        view.setWallpaper("/fixture/one");
        view.setWallpaper("/fixture/two");
        tryCompare(commit, "running", true, 300);
        compare(commit.requestPath, "/fixture/two");
        view.setWallpaper("/fixture/three");
        wait(170);
        compare(commit.requestPath, "/fixture/two");
        finish(commit, JSON.stringify(entry("/fixture/two", true)));
        compare(view.selectedPath, "/fixture/three");
        tryCompare(commit, "requestPath", "/fixture/three", 300);
        finish(commit, '{"error":"Fixture failure"}', 1);
        compare(view.selectedPath, "");
        verify(view.error.includes("failure"));
        verify(view.rotationRetryMs > Date.now());
        view.setWallpaper("/fixture/three");
        tryCompare(commit, "running", true, 300);
        finish(commit, JSON.stringify(entry("/fixture/three")));
        compare(view.actualCurrent, "/fixture/three");
        compare(view.rotationRetryMs, 0);
    }
    function test_displayed_pair_does_not_follow_preview() {
        const view = setup();
        view.media = entry("/fixture/one");
        view.lastImage = view.media.poster;
        NacrePresentation.active = {
            poster: view.media.poster
        };
        view.browse("/fixture/two");
        compare(view.current, "/fixture/two");
        verify(view.dynamic);
        compare(view.displayPath, "/fixture/one");
        verify(!view.displayDynamic);
        NacrePresentation.active = {
            poster: "/fixture/two.png"
        };
        compare(view.displayPath, "/fixture/two");
        verify(view.displayDynamic);
    }
    function test_preferences_serial_confirmed_and_stdin() {
        const view = setup(), writer = findChild(view, "wallpaperPreferences");
        view.preference({
            layout: "hexagons"
        });
        view.preference({
            paletteHarmony: true
        });
        compare(view.preferences.layout, "carousel");
        wait(1);
        compare(writer.writes.length, 1);
        compare(JSON.parse(writer.writes[0]).layout, "hexagons");
        finish(writer, JSON.stringify({
            kind: "static",
            layout: "hexagons"
        }));
        tryCompare(writer, "starts", 2, 300);
        compare(writer.command[2], "theme");
        finish(writer, '{"error":"Palette failure"}', 1);
        compare(view.preferences.layout, "hexagons");
        verify(view.error.includes("failure"));
        verify(!view.themeBusy);
    }
    function test_import_cancellation_and_invalid_path_never_commit() {
        const view = setup(), importer = findChild(view, "wallpaperImporter"), commit = findChild(view, "wallpaperCommit");
        view.setWallpaper("https://example.invalid/a.png");
        wait(170);
        verify(!commit.running);
        view.pickFiles();
        compare(importer.command[2], "pick");
        finish(importer, '{"cancelled":true}');
        verify(view.importReceived);
        view.addFiles(["file:///fixture/a b.png"]);
        compare(importer.command[2], "import");
        wait(1);
        compare(JSON.parse(importer.writes[0])[0], "file:///fixture/a b.png");
        finish(importer, '{"imported":[]}');
        verify(findChild(view, "wallpaperCatalog").running);
    }
}
