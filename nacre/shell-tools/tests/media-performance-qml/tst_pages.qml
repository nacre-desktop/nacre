import QtQuick
import QtTest
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
            visibilities: QtObject {
                property bool dashboard: true
            }
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
        actor.volumeSupported = false;
        view.volumeTo(.2, actor);
        compare(actor.volume, .8);
        actor.volumeSupported = true;
        view.volumeTo(2, actor);
        compare(actor.volume, 1);
        view.volumeTo(NaN, actor);
        compare(actor.volume, 1);
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
    function test_narrow_media_pages() {
        for (const width of [300, 508, 766, 835]) {
            const media = createTemporaryObject(page, test, {
                width,
                height: 400
            });
            wait(10);
            const title = findChild(media, "fullMediaTitle");
            verify(title.mapToItem(media, 0, 0).x + title.width <= width + .01);
        }
    }
}
