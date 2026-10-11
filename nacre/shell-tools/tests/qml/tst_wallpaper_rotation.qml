import QtQuick
import QtTest
import "fixtures"
import "wallpaper-rotation.js" as Rotation

TestCase {
    id: test

    function cleanup() {
        NacreWelcomeApp.active = false;
        NacreWelcomeApp.page = "home";
        WallpaperPlayback.sleeping = false;
        WallpaperPlayback.locked = false;
        NacrePanelState.screens = ({});
        NacrePanelState.settingsVisible = false;
        NacrePanelState.settingsPage = "appearance";
    }

    function test_theme_queue_coalesces_latest_values_without_mixing_actions() {
        const view = createTemporaryObject(walls, test);
        view.prefWorker.running = true;
        view.preference({
            palettePersonality: "harmony"
        });
        view.preference({
            palettePersonality: "pop"
        });
        view.preference({
            paletteMode: "light"
        });
        compare(view.preferenceQueue.length, 1);
        compare(view.preferenceQueue[0].palettePersonality, "pop");
        compare(view.preferenceQueue[0].paletteMode, "light");
        view.preference({
            layout: "spotlight"
        });
        view.preference({
            kind: "dynamic"
        });
        compare(view.preferenceQueue.length, 2);
        compare(view.preferenceQueue[1].layout, "spotlight");
        compare(view.preferenceQueue[1].kind, "dynamic");
    }
    function test_shuffle_visits_each_scene_before_repeating() {
        const pool = ["a", "b", "c", "d"];
        let bag = [], current = "a", seen = [];
        for (let i = 0; i < 3; i++) {
            const next = Rotation.next(pool, current, true, bag, 0.5);
            verify(next.path !== current);
            verify(!seen.includes(next.path));
            seen.push(next.path);
            current = next.path;
            bag = next.remaining;
        }
        compare(seen.length, 3);
        compare(Rotation.next(["a"], "a", true, [], 0.5).path, "");
    }

    function test_filter_and_sequential_order() {
        const entries = [
            {
                "path": "a",
                "dynamic": false
            },
            {
                "path": "b",
                "dynamic": true
            }
        ];
        compare(Rotation.pool(entries, "dynamic").join(","), "b");
        compare(Rotation.pool(entries, "static").join(","), "a");
        compare(Rotation.next(["a", "b", "c"], "c", false, [], 0.5).path, "a");
    }

    function test_restore_pauses_rotation_without_resetting_deadline() {
        const view = createTemporaryObject(walls, test);
        view.preferences = Object.assign({}, view.preferences, {
            rotationEnabled: true
        });
        const deadline = view.rotationDueMs;
        view.demoRestore.running = true;
        verify(view.themeBusy);
        verify(!view.canRotate);
        compare(view.rotationDueMs, deadline);
        view.demoRestore.running = false;
        verify(view.canRotate);
        compare(view.rotationDueMs, deadline);
    }
    function test_welcome_demo_pauses_without_resetting_deadline() {
        const w = createTemporaryObject(walls, test);
        const due = w.rotationDueMs;
        NacreWelcomeApp.active = true;
        NacreWelcomeApp.page = "home";
        verify(!w.canRotate);
        compare(w.rotationDueMs, due);
        NacreWelcomeApp.page = "shortcuts";
        verify(w.welcomeDemoOpen);
        NacreWelcomeApp.active = false;
        NacreWelcomeApp.page = "home";
        compare(w.rotationDueMs, due);
    }

    function test_disabled_timer_and_lifecycle_pause() {
        const view = createTemporaryObject(walls, test);
        const clock = findChild(view, "wallpaperRotationTimer");
        verify(clock);
        verify(!clock.running);
        view.preferences = Object.assign({}, view.preferences, {
            "rotationEnabled": true
        });
        verify(clock.running);
        verify(clock.interval > 30 * 60000 - 1000 && clock.interval <= 30 * 60000);
        WallpaperPlayback.locked = true;
        verify(!clock.running);
        WallpaperPlayback.locked = false;
        verify(clock.running);
        WallpaperPlayback.sleeping = true;
        verify(!clock.running);
        WallpaperPlayback.sleeping = false;
        verify(clock.running);
        NacrePanelState.screens = ({
                "test": {
                    "launcher": true,
                    "launcherMode": "wallpaper"
                }
            });
        verify(!clock.running);
        NacrePanelState.screens = ({});
        NacrePanelState.settingsVisible = true;
        verify(!clock.running);
        NacrePanelState.settingsPage = "sound";
        verify(clock.running);
        NacrePanelState.settingsPage = "appearance";
        verify(!clock.running);
        NacrePanelState.settingsVisible = false;
        verify(clock.running);
        view.preferences = Object.assign({}, view.preferences, {
            "rotationMinutes": 60
        });
        verify(clock.interval > 60 * 60000 - 1000 && clock.interval <= 60 * 60000);
        view.preferences = Object.assign({}, view.preferences, {
            "rotationEnabled": false
        });
        verify(!clock.running);
    }

    function test_timer_uses_single_existing_commit_queue() {
        const view = createTemporaryObject(walls, test);
        view.preferences = Object.assign({}, view.preferences, {
            "rotationEnabled": true,
            "rotationShuffle": false,
            "rotationAnchorMs": Date.now() - 30 * 60000 + 50
        });
        const clock = findChild(view, "wallpaperRotationTimer");
        tryCompare(view.commit, "running", true, 300);
        compare(view.commit.requestPath, "a");
        verify(!clock.running);
        view.advanceRotation(true);
        compare(view.commit.requestPath, "a");
    }

    function test_temporary_pauses_and_unrelated_preferences_keep_deadline() {
        const view = createTemporaryObject(walls, test);
        const anchor = Date.now();
        view.preferences = Object.assign({}, view.preferences, {
            "rotationEnabled": true,
            "rotationAnchorMs": anchor
        });
        const due = view.rotationDueMs;
        const clock = findChild(view, "wallpaperRotationTimer");
        wait(60);
        WallpaperPlayback.locked = true;
        wait(60);
        WallpaperPlayback.locked = false;
        compare(view.rotationDueMs, due);
        verify(clock.interval < 30 * 60000 - 90);
        view.preferences = Object.assign({}, view.preferences, {
            "layout": "hexagons",
            "palettePreset": "ocean"
        });
        compare(view.rotationDueMs, due);
        verify(clock.interval < 30 * 60000 - 90);
    }

    function test_renderer_restart_keeps_saved_schedule() {
        const anchor = Date.now() - 10 * 60000;
        const saved = {
            "rotationEnabled": true,
            "rotationMinutes": 30,
            "rotationAnchorMs": anchor
        };
        const first = createTemporaryObject(walls, test);
        first.preferences = saved;
        const second = createTemporaryObject(walls, test);
        second.preferences = saved;
        compare(first.rotationDueMs, anchor + 30 * 60000);
        compare(second.rotationDueMs, first.rotationDueMs);
        verify(findChild(second, "wallpaperRotationTimer").interval <= 20 * 60000);
    }

    function test_overdue_pause_rotates_once_after_resume() {
        WallpaperPlayback.locked = true;
        const view = createTemporaryObject(walls, test);
        view.preferences = {
            "rotationEnabled": true,
            "rotationMinutes": 30,
            "rotationShuffle": false,
            "rotationAnchorMs": Date.now() - 30 * 60000 + 30
        };
        wait(80);
        verify(!view.commit.running);
        WallpaperPlayback.locked = false;
        tryCompare(view.commit, "running", true, 400);
        compare(view.commit.requestPath, "a");
        view.media = {
            "path": "a",
            "poster": "a",
            "appliedAtMs": Date.now()
        };
        view.lastImage = "a";
        view.selectedPath = "";
        view.commit.running = false;
        wait(80);
        verify(!view.commit.running);
        verify(findChild(view, "wallpaperRotationTimer").running);
        verify(view.rotationDueMs > Date.now() + 29 * 60000);
    }

    function test_palette_publication_without_photo_change_keeps_deadline() {
        const view = createTemporaryObject(walls, test);
        const changed = Date.now() - 50000;
        view.media = {
            "path": "a",
            "poster": "a",
            "appliedAtMs": changed
        };
        view.lastImage = "a";
        view.preferences = {
            "rotationEnabled": true,
            "rotationMinutes": 30,
            "rotationAnchorMs": changed - 10000
        };
        const due = view.rotationDueMs;
        NacrePresentation.active = {
            "poster": "a",
            "changedAtMs": changed,
            "colours": {
                "primary": "ffffff"
            }
        };
        compare(view.rotationDueMs, due);
        NacrePresentation.active = {
            "poster": "a",
            "changedAtMs": changed,
            "colours": {
                "primary": "abcdef"
            }
        };
        compare(view.rotationDueMs, due);
        NacrePresentation.active = ({});
    }

    function test_previous_tracks_successful_visits_and_failure_keeps_history() {
        const view = createTemporaryObject(walls, test);
        view.list = ["/a", "/b", "/c"].map(path => ({
                    path,
                    poster: path,
                    name: path,
                    dynamic: true,
                    animated: false
                }));
        view.preferences = {
            rotationShuffle: false,
            rotationEnabled: false
        };
        view.media = view.list[0];
        view.lastImage = "/a";
        function complete(index) {
            verify(view.publishSelection(JSON.stringify(view.list[index])));
            view.commit.running = false;
        }
        view.advanceRotation(true);
        compare(view.commit.requestPath, "/b");
        complete(1);
        view.advanceRotation(true);
        compare(view.commit.requestPath, "/c");
        complete(2);
        compare(view.wallpaperHistory.join(","), "/a,/b");
        view.previousWallpaper();
        compare(view.commit.requestPath, "/b");
        verify(!view.publishSelection('{}'));
        compare(view.wallpaperHistory.join(","), "/a,/b");
        complete(1);
        compare(view.wallpaperHistory.join(","), "/a");
        view.previousWallpaper();
        compare(view.commit.requestPath, "/a");
        complete(0);
        compare(view.wallpaperHistory.length, 0);
        view.previousWallpaper();
        compare(view.commit.requestPath, "/c");
        const request = view.commit.requestPath;
        view.previousWallpaper();
        compare(view.commit.requestPath, request);
    }
    function test_previous_preserves_repeated_nonadjacent_visits() {
        const view = createTemporaryObject(walls, test);
        view.list = ["/a", "/b", "/c"].map(path => ({
                    path,
                    poster: path,
                    name: path,
                    dynamic: true,
                    animated: false
                }));
        view.media = view.list[1];
        view.lastImage = "/b";
        view.wallpaperHistory = ["/a", "/b", "/a"];
        view.previousWallpaper();
        compare(view.commit.requestPath, "/a");
        verify(view.publishSelection(JSON.stringify(view.list[0])));
        view.commit.running = false;
        compare(view.wallpaperHistory.join(","), "/a,/b");
        view.previousWallpaper();
        compare(view.commit.requestPath, "/b");
        verify(view.publishSelection(JSON.stringify(view.list[1])));
        view.commit.running = false;
        compare(view.wallpaperHistory.join(","), "/a");
    }
    function test_history_keeps_original_when_poster_file_arrives_first() {
        const view = createTemporaryObject(walls, test);
        view.list = ["/a", "/b"].map(path => ({
                    path,
                    poster: path,
                    name: path,
                    dynamic: true,
                    animated: false
                }));
        view.preferences = {
            rotationShuffle: false,
            rotationEnabled: false
        };
        view.media = view.list[0];
        view.lastImage = "/a";
        view.advanceRotation(true);
        compare(view.commit.requestPath, "/b");
        // FileView can receive the new poster before the worker's final selection response.
        view.lastImage = "/new-poster.png";
        verify(view.publishSelection(JSON.stringify(view.list[1])));
        view.commit.running = false;
        compare(view.wallpaperHistory.join(","), "/a");
        view.previousWallpaper();
        compare(view.commit.requestPath, "/a");
    }

    function test_motion_toggle_uses_existing_preference_owner() {
        const view = createTemporaryObject(walls, test);
        view.media = {
            path: "/a",
            poster: "/a",
            dynamic: true,
            animated: false
        };
        view.lastImage = "/a";
        NacrePresentation.active = {
            poster: "/a"
        };
        WallpaperPlayback.paused = false;
        view.toggleMotion();
        compare(view.prefWorker.value.paused, true);
        view.prefWorker.running = false;
        view.media.dynamic = false;
        NacrePresentation.active = {};
    }

    name: "WallpaperRotation"
    when: windowShown

    Component {
        id: walls

        RotatingWalls {}
    }
}
