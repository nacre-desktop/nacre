import QtQuick
import QtTest
import "fixtures"

TestCase {
    id: test
    name: "NacreMediaSelection"
    when: windowShown
    Component {
        id: service
        NacrePlayers {}
    }
    Component {
        id: player
        QtObject {
            property string identity: "Fixture"
            property bool isPlaying: false
            property bool canPlay: true
            property bool canPause: true
            property bool canTogglePlaying: true
            property bool canControl: true
            property bool canGoNext: false
            property bool canGoPrevious: false
            property var commands: []
            function play() {
                commands = [...commands, "play"];
            }
            function pause() {
                commands = [...commands, "pause"];
            }
            function togglePlaying() {
                commands = [...commands, "toggle"];
            }
            function next() {
                commands = [...commands, "next"];
            }
            function previous() {
                commands = [...commands, "previous"];
            }
            function stop() {
                commands = [...commands, "stop"];
            }
        }
    }
    function test_priority_manual_removal_and_read_only_selection() {
        const spotify = createTemporaryObject(player, test, {
            identity: "Spotify"
        });
        const browser = createTemporaryObject(player, test, {
            identity: "Browser",
            isPlaying: true
        });
        Mpris.players = {
            values: [spotify, browser]
        };
        const state = createTemporaryObject(service, test);
        compare(state.active, browser);
        compare(browser.commands.length, 0);
        compare(spotify.commands.length, 0);
        spotify.isPlaying = true;
        compare(state.active, spotify);
        state.manualActive = browser;
        compare(state.active, browser);
        state.control("next");
        compare(browser.commands.length, 0);
        state.control("pause");
        compare(browser.commands[0], "pause");
        Mpris.players = {
            values: [spotify]
        };
        compare(state.manualActive, null);
        compare(state.active, spotify);
        state.manualActive = browser;
        compare(state.active, spotify);
        state.control("bogus");
        compare(spotify.commands.length, 0);
        spotify.canTogglePlaying = false;
        verify(!state.control("playPause"));
        Mpris.players = {
            values: []
        };
        compare(state.active, null);
        verify(!state.control("play"));
    }
}
