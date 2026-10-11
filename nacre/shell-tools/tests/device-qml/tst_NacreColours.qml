import QtQuick
import QtTest
import "fixtures"
import "colour-data.js" as Data

TestCase {
    id: test
    name: "NacrePaletteSnapshot"
    when: windowShown
    Component {
        id: service
        NacreColours {}
    }
    function test_complete_channels_atomic_mode_and_last_good_rejection() {
        const colours = createTemporaryObject(service, test);
        const raw = Object.assign({}, Data.fallback, {
            primary: "#ff2200",
            surface: "#201000",
            onSurface: "#ffffff"
        });
        verify(colours.apply({
            mode: "dark",
            colours: raw
        }));
        verify(colours.ready);
        compare(colours.palette.m3primary.r, 1);
        verify(colours.publishing);
        compare(colours.palette.m3surface.g, 16 / 255);
        verify(colours.palette.blue.r >= 0);
        const state = colours.state;
        verify(!colours.apply({
            mode: "light",
            colours: {
                primary: "#112233"
            }
        }));
        compare(colours.state, state);
        verify(!colours.light);
        verify(colours.apply({
            mode: "light",
            colours: raw
        }));
        verify(colours.light);
        compare(colours.palette.m3primary.r, 1);
        wait(20);
        verify(!colours.publishing);
    }
    function test_committed_presentation_and_explicit_mode_request() {
        NacrePresentation.active = {};
        NacrePresentation.available = false;
        AppLaunch.calls = [];
        const colours = createTemporaryObject(service, test);
        NacrePresentation.active = {
            mode: "dark",
            colours: Data.fallback
        };
        NacrePresentation.available = true;
        verify(colours.ready);
        const raw = Object.assign({}, Data.fallback, {
            primary: "ee0000"
        });
        NacrePresentation.active = {
            mode: "dark",
            colours: raw
        };
        compare(colours.palette.m3primary.r, 238 / 255);
        compare(AppLaunch.calls.length, 0);
        verify(!colours.setMode("bogus"));
        verify(colours.setMode("light"));
        compare(AppLaunch.calls[0].command.join("|"), "nacre-shell|scheme-mode|light");
        verify(Qt.colorEqual(colours.on(Qt.rgba(1, 1, 1, 1)), "black"));
    }
}
