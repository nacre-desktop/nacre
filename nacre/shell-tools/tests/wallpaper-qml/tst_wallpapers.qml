import QtQuick
import QtTest
import "fixtures"
import "colour-data.js" as Palette

TestCase {
    id: test
    name: "WallpaperPicker"
    width: 1160
    height: 650
    visible: true
    when: windowShown
    Component {
        id: picker
        NacreWallpaperPicker {
            width: 1160
            height: implicitHeight
            visibilities: QtObject {
                property bool launcher: true
                property bool previewOnly: false
            }
        }
    }
    Component {
        id: presentation
        NacrePresentation {}
    }
    Component {
        id: backdrop
        NacreWallpaperBackdrop {
            width: 500
            height: 300
        }
    }
    Component {
        id: hex
        NacreWallpaperHex {
            entry: NacreWallpapers.list[0]
        }
    }
    function init() {
        NacreWallpapers.preferences = {
            kind: "static",
            layout: "carousel"
        };
        NacreWallpapers.current = "one";
        NacreWallpapers.browsed = "";
    }
    Component {
        id: motionPreview
        NacreWallpaperMotion {
            width: 300
            height: 180
        }
    }
    function test_gallery_backdrop_fills_viewport_and_separates_modes() {
        const view = createTemporaryObject(picker, test);
        const divider = findChild(view, "wallpaperModeDivider");
        verify(!!divider);
        const media = findChild(view, "wallpaperMediaChoices");
        const layouts = findChild(view, "wallpaperLayoutChoices");
        wait(40);
        const a = media.mapToItem(view, media.width, 0);
        const b = layouts.mapToItem(view, 0, 0);
        verify(b.x > a.x);
        NacreWallpapers.preference({
            layout: "spotlight"
        });
        tryCompare(findChild(view, "galleryScrim"), "visible", true);
        compare(findChild(view, "galleryScrim").width, view.width);
        const background = findChild(view, "galleryBackdrop");
        compare(background.width, view.width);
        compare(background.height, view.height);
        tryCompare(background, "hasImage", true);
        view.move(1);
        compare(NacreWallpapers.browsed, "two");
        verify(view.visibilities.launcher);
    }
    function test_preview_navigation_does_not_apply_wallpaper() {
        const view = createTemporaryObject(picker, test);
        view.visibilities.previewOnly = true;
        view.move(1);
        compare(view.currentIndex, 1);
        compare(NacreWallpapers.browsed, "");
        view.choose();
        compare(NacreWallpapers.current, "one");
    }
    function test_preview_cancels_decode_when_selection_moves_before_settling() {
        const view = createTemporaryObject(motionPreview, test, {
            entry: {
                path: "unused.mp4",
                dynamic: true
            },
            running: true
        });
        wait(60);
        verify(!view.active);
        view.running = false;
        wait(220);
        verify(!view.active);
    }
    function test_wheel_packets_are_accumulated_and_empty_packets_ignored() {
        const view = createTemporaryObject(picker, test);
        view.visibilities.previewOnly = true;
        NacreWallpapers.preference({
            layout: "spotlight"
        });
        const packet = {
            pixelDelta: {
                x: 0,
                y: -20
            },
            angleDelta: {
                x: 0,
                y: 0
            },
            accepted: false
        };
        view.wheelStep(packet);
        compare(view.currentIndex, 0);
        view.wheelStep(packet);
        compare(view.currentIndex, 0);
        view.wheelStep(packet);
        compare(view.currentIndex, 1);
        view.wheelStep({
            pixelDelta: {
                x: 0,
                y: 0
            },
            angleDelta: {
                x: 0,
                y: 0
            }
        });
        compare(view.currentIndex, 1);
        compare(NacreWallpapers.browsed, "");
    }
    function test_backdrop_keeps_an_opaque_underlayer_during_fade() {
        const first = NacreWallpapers.list[0].poster;
        const view = createTemporaryObject(backdrop, test, {
            path: first
        });
        tryCompare(view, "hasImage", true);
        const old = view.current;
        view.path = first.replace("poster.png", "second.png");
        tryVerify(() => view.current !== old);
        wait(50);
        compare(old.opacity, 1);
        verify(view.current.opacity > 0 && view.current.opacity < 1);
        verify(view.current.z > old.z);
        wait(400);
        compare(view.current.opacity, 1);
        compare(old.opacity, 0);
    }
    function test_gallery_motion_respects_pause_and_close() {
        const view = createTemporaryObject(picker, test);
        NacreWallpapers.preference({
            kind: "dynamic",
            paused: true
        });
        verify(!view.motionEnabled);
        NacreWallpapers.preference({
            paused: false
        });
        verify(view.motionEnabled);
        view.visibilities.launcher = false;
        verify(!view.motionEnabled);
    }
    function test_static_dynamic_filter_and_search() {
        const view = createTemporaryObject(picker, test);
        wait(40);
        compare(view.count, 2);
        const input = findChild(view, "wallpaperSearch");
        input.text = "hollow";
        compare(view.count, 1);
        compare(view.currentEntry.path, "two");
        compare(NacreWallpapers.browsed, "");
        view.choose();
        compare(NacreWallpapers.browsed, "two");
        input.text = "";
        NacreWallpapers.preference({
            kind: "dynamic"
        });
        compare(view.count, 1);
        compare(view.currentEntry.path, "three");
    }
    function test_layout_switch_preserves_selection_and_escape() {
        const view = createTemporaryObject(picker, test);
        wait(40);
        view.select(1);
        compare(view.currentEntry.path, "two");
        for (const layout of ["spotlight", "hexagons", "carousel"]) {
            NacreWallpapers.preference({
                layout: layout
            });
            wait(40);
            compare(view.currentEntry.path, "two");
        }
        view.forceActiveFocus();
        keyClick(Qt.Key_Escape);
        compare(view.visibilities.launcher, false);
    }
    function test_repeated_layout_switch_during_carousel_animation() {
        const view = createTemporaryObject(picker, test);
        wait(40);
        for (let i = 0; i < 12; i++) {
            NacreWallpapers.preference({
                layout: "carousel"
            });
            view.select(i % 2);
            wait(10);
            NacreWallpapers.preference({
                layout: "spotlight"
            });
            wait(10);
            compare(view.currentEntry.path, i % 2 === 0 ? "one" : "two");
        }
    }
    function test_full_screen_modes_and_complete_carousel_cards() {
        const original = NacreWallpapers.list;
        try {
            NacreWallpapers.list = Array.from({
                length: 12
            }, (_, i) => ({
                        path: "wall" + i,
                        name: "Wallpaper " + i,
                        poster: original[0].poster,
                        dynamic: false
                    }));
            const view = createTemporaryObject(picker, test);
            wait(60);
            verify(!view.fullScreen);
            verify(view.implicitHeight >= 300 && view.implicitHeight <= 340);
            const strip = findChild(view, "carouselStrip");
            verify(strip.slots % 2 === 1);
            for (let i = 0; i < 12; i++) {
                const card = findChild(view, "carouselCard" + i);
                if (!card || !card.visible)
                    continue;
                const point = card.mapToItem(strip, 0, 0), end = card.mapToItem(strip, card.width, card.height);
                verify(point.x >= 0, "left " + point.x);
                verify(end.x <= strip.width + 1, "right " + end.x + " / " + strip.width);
                verify(point.y >= 0, "top " + point.y);
                verify(end.y <= strip.height + 1, "bottom " + end.y + " / " + strip.height);
            }
            for (const layout of ["spotlight", "hexagons"]) {
                NacreWallpapers.preference({
                    layout: layout
                });
                wait(40);
                verify(view.fullScreen);
                compare(view.implicitHeight, 1100);
            }
        } finally {
            NacreWallpapers.list = original;
        }
    }
    function test_open_and_reopen_start_centered_without_initial_travel() {
        const original = NacreWallpapers.list;
        try {
            NacreWallpapers.list = Array.from({
                length: 9
            }, (_, i) => ({
                        path: "wall" + i,
                        name: "Wallpaper " + i,
                        poster: original[0].poster,
                        dynamic: false
                    }));
            for (const layout of ["carousel", "spotlight"]) {
                NacreWallpapers.preference({
                    layout: layout
                });
                NacreWallpapers.current = "wall6";
                const view = createTemporaryObject(picker, test);
                compare(view.currentIndex, 6);
                compare(view.travel, 6);
                wait(50);
                compare(view.travel, 6);
                view.visibilities.previewOnly = true;
                view.move(1);
                wait(50);
                verify(view.travel > 6 && view.travel < 7);
                view.visibilities.launcher = false;
                NacreWallpapers.current = "wall3";
                view.visibilities.launcher = true;
                compare(view.currentIndex, 3);
                compare(view.travel, 3);
                view.destroy();
            }
        } finally {
            NacreWallpapers.list = original;
        }
    }
    function test_late_current_path_snaps_but_does_not_override_user_browsing() {
        const view = createTemporaryObject(picker, test);
        NacreWallpapers.current = "two";
        compare(view.currentIndex, 1);
        compare(view.travel, 1);
        view.visibilities.previewOnly = true;
        view.move(1);
        compare(view.currentIndex, 0);
        NacreWallpapers.current = "two";
        compare(view.currentIndex, 0);
    }
    function test_late_catalogue_reconciles_without_animating_from_zero() {
        const original = NacreWallpapers.list;
        try {
            NacreWallpapers.current = "two";
            NacreWallpapers.list = [];
            const view = createTemporaryObject(picker, test);
            NacreWallpapers.list = original;
            compare(view.currentIndex, 1);
            compare(view.travel, 1);
            wait(40);
            compare(view.travel, 1);
        } finally {
            NacreWallpapers.list = original;
        }
    }
    function test_carousel_slides_through_intermediate_position() {
        const original = NacreWallpapers.list;
        try {
            NacreWallpapers.list = Array.from({
                length: 9
            }, (_, i) => ({
                        path: "wall" + i,
                        name: "Wallpaper " + i,
                        poster: original[0].poster,
                        dynamic: false
                    }));
            const view = createTemporaryObject(picker, test);
            wait(400);
            const strip = findChild(view, "carouselStrip"), next = findChild(view, "carouselCard1");
            const start = next.x, end = (strip.width - next.width) / 2;
            view.move(1);
            wait(110);
            verify(next.x < start && next.x > end);
            wait(300);
            fuzzyCompare(next.x, end, 1);
        } finally {
            NacreWallpapers.list = original;
        }
    }
    function test_spotlight_motion_has_intermediate_position_and_width() {
        const original = NacreWallpapers.list;
        try {
            NacreWallpapers.list = Array.from({
                length: 9
            }, (_, i) => ({
                        path: "wall" + i,
                        name: "Wallpaper " + i,
                        poster: original[0].poster,
                        dynamic: false
                    }));
            NacreWallpapers.preference({
                layout: "spotlight"
            });
            const view = createTemporaryObject(picker, test);
            wait(400);
            const strip = findChild(view, "spotlightStrip"), next = findChild(view, "spotlightCard1");
            const startX = next.x, startWidth = next.width, endX = (strip.width - strip.heroWidth) / 2;
            view.select(1);
            wait(110);
            verify(next.x < startX && next.x > endX);
            verify(next.width > startWidth && next.width < strip.heroWidth);
            wait(300);
            compare(next.x, endX);
            compare(next.width, strip.heroWidth);
        } finally {
            NacreWallpapers.list = original;
        }
    }
    function test_theme_waits_for_matching_image_ready_and_rejects_stale_ack() {
        const view = createTemporaryObject(presentation, test);
        const first = {
            poster: "/fixture/one",
            mode: "dark",
            colours: Object.assign({}, Palette.fallback, {
                primary: "112233"
            })
        }, next = {
            poster: "/fixture/two",
            mode: "light",
            colours: Object.assign({}, Palette.fallback, {
                primary: "aabbcc"
            })
        };
        view.accept(first);
        compare(view.active.poster, "/fixture/one");
        view.accept(next);
        compare(view.active.poster, "/fixture/one");
        view.activate("/fixture/one");
        compare(view.active.poster, "/fixture/one");
        view.activate("/fixture/two");
        compare(view.active.poster, "/fixture/two");
        compare(view.active.colours.primary, "aabbcc");
        view.accept({
            poster: "/fixture/two",
            mode: "dark",
            colours: Object.assign({}, Palette.fallback, {
                primary: "445566"
            })
        });
        compare(view.active.mode, "dark");
    }
    function test_backdrop_holds_loaded_image_until_replacement_ready() {
        const path = NacreWallpapers.list[0].poster;
        const view = createTemporaryObject(backdrop, test, {
            path: path
        });
        tryCompare(view, "hasImage", true);
        const previous = view.current;
        view.path = path.replace("poster.png", "second.png");
        compare(view.current, previous);
        wait(650);
        verify(view.current !== previous);
        compare(view.current.status, Image.Ready);
        view.path = path;
        wait(650);
        compare(view.current, previous);
        verify(view.hasImage);
    }
    function test_hexagonal_hit_shape_excludes_transparent_corners() {
        const view = createTemporaryObject(hex, test);
        verify(view.inside(100, 80));
        verify(!view.inside(0, 0));
        verify(!view.inside(199, 1));
        verify(view.inside(1, 87));
    }
}
