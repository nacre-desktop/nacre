import QtQuick
import QtTest
import Quickshell
import "fixtures"
import "media/media.js" as Model

TestCase {
    id: test
    name: "NacreMediaPerformance"
    width: 1100
    height: 1100
    visible: true
    when: windowShown
    Component {
        id: page
        NacreMediaPage {
            shouldUpdate: true
            visibilities: PersistentProperties {
                property bool dashboard: true
            }
        }
    }
    Component {
        id: weatherSky
        NacreWeatherSky {
            width: 564
            height: 320
            active: true
        }
    }
    Component {
        id: focusPage
        NacreFocusExtra {
            width: 940
            height: 562
            shouldUpdate: true
        }
    }
    Component {
        id: player
        QtObject {
            property string identity: "Fixture player"
            property string trackTitle: "Fixture track"
            property string trackAlbum: "Fixture album"
            property string trackArtist: "Fixture artist"
            property string trackArtUrl: ""
            property int uniqueId: 7
            property real position: 120
            property real length: 400
            property bool positionSupported: true
            property bool lengthSupported: true
            property bool canSeek: true
            property bool canControl: true
            property bool canGoPrevious: true
            property bool canGoNext: true
            property bool canTogglePlaying: true
            property bool canRaise: true
            property bool isPlaying: true
            property bool shuffleSupported: true
            property bool shuffle: false
            property bool loopSupported: true
            property int loopState: 0
            property bool volumeSupported: true
            property real volume: .8
            property int nextCalls: 0
            property int previousCalls: 0
            property int toggleCalls: 0
            property int raiseCalls: 0
            property int writes: 0
            signal trackChanged
            signal postTrackChanged
            onPositionChanged: writes++
            function previous() {
                previousCalls++;
            }
            function next() {
                nextCalls++;
            }
            function togglePlaying() {
                toggleCalls++;
                isPlaying = !isPlaying;
            }
            function raise() {
                raiseCalls++;
            }
        }
    }
    function init() {
        NacreDashboardState.focusState = {
            status: "idle"
        };
        NacreDashboardState.calls = [];
        NacreMediaDetails.lines = [];
        NacreMediaDetails.plain = "";
        NacreMediaDetails.spectrum = [];
        NacreDashboardState.preferences = {
            albumPalette: true
        };
        NacreAudio.outputs = [];
        NacreAudio.sink = null;
        NacrePlayers.list = [];
        NacrePlayers.active = null;
        NacrePlayers.manualActive = null;
        DesktopSettings.data = {
            animations: false
        };
    }
    function setup() {
        const actor = createTemporaryObject(player, test);
        NacrePlayers.list = [actor];
        NacrePlayers.active = actor;
        actor.writes = 0;
        const view = createTemporaryObject(page, test);
        wait(20);
        return {
            actor,
            view
        };
    }
    function test_empty_and_formatting_without_writes() {
        const view = createTemporaryObject(page, test);
        wait(20);
        compare(findChild(view, "fullMediaTitle").text, "No media playing");
        compare(view.canSeek, false);
        compare(view.sampling, false);
        compare(Model.duration(61.9), "1:01");
        compare(Model.duration(7201), "2:00:01");
        compare(Model.duration(NaN), "—");
        compare(Model.size(16 * 1048576), "16.0 GiB");
        compare(Model.temperature(0), "0°C");
        const {
            actor,
            view: live
        } = setup();
        compare(actor.writes, 0);
        compare(live.progress, .3);
        actor.position = 80;
        compare(actor.writes, 1);
        compare(live.progress, .2);
        live.refresh();
        compare(actor.writes, 1);
    }
    function test_pointer_transport_caps_selection_and_raise() {
        const {
            actor,
            view
        } = setup();
        mouseClick(findChild(view, "fullPrevious"), 21, 21);
        mouseClick(findChild(view, "fullToggle"), 24, 24);
        mouseClick(findChild(view, "fullNext"), 21, 21);
        compare(actor.previousCalls, 1);
        compare(actor.toggleCalls, 1);
        compare(actor.nextCalls, 1);
        actor.canControl = false;
        view.perform("next");
        view.perform("shuffle");
        compare(actor.nextCalls, 1);
        compare(actor.shuffle, false);
        actor.canControl = true;
        view.perform("shuffle");
        compare(actor.shuffle, true);
        view.perform("repeat");
        compare(actor.loopState, 1);
        view.perform("repeat");
        compare(actor.loopState, 2);
        view.perform("repeat");
        compare(actor.loopState, 0);
        const second = createTemporaryObject(player, test, {
            identity: "Second player"
        });
        NacrePlayers.list = [actor, second];
        view.choose(second);
        compare(NacrePlayers.active, second);
        NacrePlayers.list = [actor];
        NacrePlayers.active = actor;
        view.choose(second);
        compare(NacrePlayers.active, actor);
        actor.canRaise = false;
        view.raisePlayer();
        compare(actor.raiseCalls, 0);
        verify(view.visibilities.dashboard);
        actor.canRaise = true;
        view.raisePlayer();
        compare(actor.raiseCalls, 1);
        compare(view.visibilities.dashboard, false);
    }
    function test_seek_commits_once_on_pointer_release_and_keyboard() {
        const {
            actor,
            view
        } = setup();
        const slider = findChild(view, "fullSeek");
        mousePress(slider, slider.width * .6, 17);
        mouseMove(slider, slider.width * .8, 17);
        verify(slider.pressed);
        compare(view.sampling, false);
        compare(actor.writes, 0);
        mouseRelease(slider, slider.width * .8, 17);
        compare(actor.writes, 1);
        verify(actor.position > 300 && actor.position < 340);
        slider.forceActiveFocus();
        keyClick(Qt.Key_Left);
        verify(actor.writes >= 2);
    }
    function test_seek_cancels_changed_track_player_and_capability() {
        const {
            actor,
            view
        } = setup();
        const slider = findChild(view, "fullSeek");
        mousePress(slider, slider.width * .4, 17);
        mouseMove(slider, slider.width * .7, 17);
        actor.uniqueId++;
        actor.trackChanged();
        mouseRelease(slider, slider.width * .7, 17);
        compare(actor.writes, 0);
        mousePress(slider, slider.width * .4, 17);
        mouseMove(slider, slider.width * .7, 17);
        const second = createTemporaryObject(player, test);
        second.writes = 0;
        NacrePlayers.list = [second];
        NacrePlayers.active = second;
        mouseRelease(slider, slider.width * .7, 17);
        compare(actor.writes, 0);
        compare(second.writes, 0);
        view.beginSeek();
        second.canSeek = false;
        view.seekTo(.9, second, second.uniqueId);
        compare(second.writes, 0);
        second.canSeek = true;
        second.length = 0;
        compare(view.canSeek, false);
    }
    function test_visibility_missing_length_and_removal() {
        const {
            actor,
            view
        } = setup();
        verify(view.sampling);
        view.shouldUpdate = false;
        compare(view.sampling, false);
        view.perform("next");
        compare(actor.nextCalls, 0);
        view.shouldUpdate = true;
        actor.isPlaying = false;
        compare(view.sampling, false);
        actor.lengthSupported = false;
        compare(view.canSeek, false);
        compare(view.progress, 0);
        NacrePlayers.list = [];
        NacrePlayers.active = null;
        compare(view.positionSeconds, 0);
        compare(view.artwork, "");
        compare(view.sampling, false);
    }
    function test_layout_fits_attached_and_small_pearl() {
        const {
            actor,
            view
        } = setup();
        for (const size of [[940, 562], [516, 472], [300, 400], [1400, 800]]) {
            view.width = size[0];
            view.height = size[1];
            wait(20);
            for (const name of ["fullAlbum", "fullMediaTitle", "mediaTransport", "mediaSecondary", "fullPlayerChoices"]) {
                const item = findChild(view, name);
                const point = item.mapToItem(view, 0, 0);
                verify(point.x >= 0 && point.y >= 0, name + " begins inside the page");
                verify(point.x + item.width <= view.width + .01, name + " fits width");
                verify(point.y + item.height <= view.height + .01, name + " fits height");
            }
            const title = findChild(view, "fullMediaTitle");
            const transport = findChild(view, "mediaTransport");
            const metadata = findChild(view, "mediaMetadata");
            verify(title.mapToItem(view, 0, title.height).y <= transport.mapToItem(view, 0, 0).y, "metadata stays above controls");
            compare(transport.y - metadata.y - metadata.height, 16, "artwork and transport stay grouped at every size");
        }
    }
    function test_spectrum_lease_stops_when_hidden() {
        const {
            view
        } = setup();
        compare(NacreMediaDetails.mediaSpectrumUsers, 1);
        view.shouldUpdate = false;
        compare(NacreMediaDetails.mediaSpectrumUsers, 0);
        view.shouldUpdate = true;
        compare(NacreMediaDetails.mediaSpectrumUsers, 1);
        view.visible = false;
        compare(NacreMediaDetails.mediaSpectrumUsers, 0);
        view.visible = true;
        compare(NacreMediaDetails.mediaSpectrumUsers, 1);
    }
    function test_activity_bars_follow_audio_and_stop_for_pause_and_reduced_motion() {
        const {
            actor,
            view
        } = setup();
        DesktopSettings.data = {
            animations: true
        };
        const bar = findChild(view, "mediaSpectrumBar0");
        const spectrum = findChild(view, "mediaSpectrum");
        NacreMediaDetails.spectrum = Array(32).fill(.8);
        verify(bar.height > spectrum.height * .7);
        NacreMediaDetails.spectrum = Array(32).fill(.2);
        verify(bar.height < spectrum.height * .3);
        actor.isPlaying = false;
        compare(bar.height, 2);
        actor.isPlaying = true;
        DesktopSettings.data = {
            reduceMotion: true
        };
        compare(spectrum.active, false);
        compare(bar.height, 2);
        DesktopSettings.data = {
            animations: true
        };
        verify(spectrum.active);
        view.shouldUpdate = false;
        compare(bar.height, 2);
    }
    function test_cached_city_photo_ready_on_creation_and_reopening() {
        NacreDashboardState.weatherPhotos = true;
        NacreWeatherPhoto.image = Qt.resolvedUrl("city.png").toString() + "?v=123";
        for (let i = 0; i < 3; i++) {
            const sky = weatherSky.createObject(test);
            verify(sky.photoReady, "cached local photo is ready before the first frame");
            const image = findChild(sky, "weatherCityPhoto");
            sky.x = i * 50;
            sky.y = i * 30;
            compare(image.status, Image.Ready);
            sky.destroy();
            wait(1);
        }
        NacreDashboardState.weatherPhotos = false;
        const sky = createTemporaryObject(weatherSky, test);
        compare(sky.photoReady, false);
        compare(findChild(sky, "weatherCityPhoto").source.toString(), "");
    }
    function test_focus_presets_select_then_explicit_start_and_pause_stop() {
        const view = createTemporaryObject(focusPage, test);
        mouseClick(findChild(view, "focusDuration50"), 20, 15);
        compare(NacreDashboardState.calls.length, 0);
        compare(findChild(view, "focusTimer").text, "50:00");
        mouseClick(findChild(view, "focusStart"), 20, 15);
        compare(NacreDashboardState.calls[0], 50);
        wait(20);
        mouseClick(findChild(view, "focusPause"), 20, 15);
        compare(NacreDashboardState.calls[1], "focus-pause");
        wait(20);
        mouseClick(findChild(view, "focusPause"), 20, 15);
        compare(NacreDashboardState.calls[2], "focus-resume");
        wait(20);
        mouseClick(findChild(view, "focusStop"), 20, 15);
        compare(NacreDashboardState.calls[3], "stop");
    }
    function test_browser_metadata_and_plain_lyrics() {
        compare(Model.lyricsMetadata("Northbound - Quiet horizon (Official Audio)", "Northbound - Topic", "", true), {
            title: "Quiet horizon",
            artist: "Northbound",
            album: ""
        });
        compare(Model.lyricsMetadata("A documentary - Part two", "Studio", "", true).title, "A documentary - Part two");
        compare(Model.lyricsMetadata("Quiet horizon (Official Audio)", "Northbound", "Tide", false).title, "Quiet horizon (Official Audio)");
        const {
            view
        } = setup();
        NacreMediaDetails.plain = "First line\nSecond line";
        verify(findChild(view, "mediaPlainLyrics").visible);
        verify(!findChild(view, "mediaTrackDetails").visible);
        NacreMediaDetails.plain = "";
        verify(findChild(view, "mediaTrackDetails").visible);
    }
    function test_lyrics_follow_position_and_details_replace_them() {
        const {
            actor,
            view
        } = setup();
        NacreMediaDetails.lines = [
            {
                time: 0,
                text: "First"
            },
            {
                time: 100,
                text: "Second"
            },
            {
                time: 200,
                text: "Third"
            }
        ];
        const lyrics = findChild(view, "mediaLyrics");
        compare(view.lyricIndex, 1);
        compare(lyrics.currentIndex, 1);
        actor.position = 220;
        compare(lyrics.currentIndex, 2);
        NacreMediaDetails.lines = [];
        compare(lyrics.visible, false);
        verify(findChild(view, "mediaTrackDetails").visible);
    }
}
