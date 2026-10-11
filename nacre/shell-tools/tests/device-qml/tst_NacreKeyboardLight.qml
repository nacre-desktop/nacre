import QtQuick
import QtTest
import "fixtures"

TestCase {
    id: test
    name: "NacreKeyboardSteps"
    when: windowShown
    Component {
        id: service
        NacreKeyboardLight {}
    }
    function test_levels_cycle_no_initial_write_and_closed_timer() {
        NacreBrightness.hardware = {
            keyboard: {
                kind: "keyboard",
                device: "fixture::kbd_backlight",
                maximum: 3,
                current: 0
            }
        };
        const keys = createTemporaryObject(service, test);
        verify(keys.available);
        compare(keys.brightness, 0);
        verify(!findChild(keys, "keyboardLightTimer").running);
        keys.step("up");
        compare(keys.brightness, 1 / 3);
        keys.step("up");
        compare(keys.brightness, 2 / 3);
        keys.step("up");
        compare(keys.brightness, 1);
        keys.step("up");
        compare(keys.brightness, 0);
        keys.step("down");
        compare(keys.brightness, 0);
        NacreBrightness.hardware = {
            keyboard: null
        };
        verify(!keys.available);
        verify(!keys.setBrightness(.5));
        NacreBrightness.controlsVisible = false;
    }
}
