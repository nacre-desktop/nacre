import QtQuick
import QtTest
import "colour-data.js" as Palette

TestCase {
    id: test
    name: "NacreMatchedPresentation"
    when: windowShown
    Component {
        id: service
        NacrePresentation {}
    }
    function record(poster, primary) {
        return {
            poster: poster,
            mode: "dark",
            colours: Object.assign({}, Palette.fallback, {
                primary: primary || "aabbcc"
            }),
            paletteOptions: [],
            selectedAccent: null,
            changedAtMs: 1000
        };
    }
    function test_bootstrap_same_poster_and_only_latest_ready_can_activate() {
        const state = createTemporaryObject(service, test);
        verify(!state.available);
        verify(state.accept(record("/fixture/first")));
        compare(state.active.poster, "/fixture/first");
        verify(state.accept(record("/fixture/second")));
        compare(state.active.poster, "/fixture/first");
        verify(!state.activate("/fixture/first"));
        verify(state.activate("file:///fixture/second"));
        compare(state.active.poster, "/fixture/second");
        verify(state.accept(record("/fixture/second", "112233")));
        compare(state.active.colours.primary, "112233");
        state.accept(record("/fixture/third"));
        state.accept(record("/fixture/fourth"));
        verify(!state.activate("/fixture/third"));
        compare(state.active.poster, "/fixture/second");
        verify(state.activate("/fixture/fourth"));
    }
    function test_delayed_file_read_cannot_undo_a_newer_streamed_palette() {
        const state = createTemporaryObject(service, test);
        const earlier = Object.assign(record("/fixture/first"), {
            publishedAtMs: 1000
        });
        const newer = Object.assign(record("/fixture/second"), {
            publishedAtMs: 1001
        });
        verify(state.accept(earlier));
        verify(state.accept(newer));
        const revision = state.revision;
        state.error = "Earlier invalid read";
        verify(state.accept(newer));
        compare(state.error, "");
        compare(state.revision, revision);
        verify(!state.accept(earlier));
        compare(state.pending.poster, "/fixture/second");
        compare(state.active.poster, "/fixture/first");
        verify(state.activate("/fixture/second"));
        compare(state.active.poster, "/fixture/second");
    }
    function test_reject_partial_malformed_and_caller_mutation() {
        const state = createTemporaryObject(service, test);
        const input = record("/fixture/path with space%");
        state.accept(input);
        input.colours.primary = "000000";
        compare(state.active.colours.primary, "aabbcc");
        const prior = state.state;
        verify(!state.accept({
            poster: "/fixture/bad",
            mode: "dark",
            colours: {
                primary: "aabbcc"
            }
        }));
        compare(state.state, prior);
        verify(!state.load("broken"));
        compare(state.state, prior);
        verify(!state.accept(record("https://example.invalid/remote")));
        compare(state.state, prior);
        compare(state.canonicalPoster("/fixture/literal%20name"), "/fixture/literal%20name");
        compare(state.canonicalPoster("file:///fixture/space%20name"), "/fixture/space name");
        compare(state.canonicalPoster("file://otherhost/fixture/name"), "");
    }
}
