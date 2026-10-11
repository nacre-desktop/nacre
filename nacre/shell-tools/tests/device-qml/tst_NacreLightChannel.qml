import QtQuick
import QtTest

TestCase {
    id: test
    name: "NacreLightWrites"
    when: windowShown
    Component {
        id: service
        NacreLightChannel {}
    }
    function test_read_only_mount_latest_write_stale_reply_and_disconnection() {
        const descriptor = {
            kind: "backlight",
            device: "fixture",
            maximum: 100,
            current: 50
        };
        const light = createTemporaryObject(service, test, {
            descriptor: descriptor
        });
        compare(light.brightness, .5);
        verify(light.available);
        compare(light.writer.starts, 0);
        light.setBrightness(.2);
        light.setBrightness(.3);
        light.setBrightness(.4);
        wait(80);
        compare(light.writer.starts, 1);
        compare(light.writer.command[light.writer.command.length - 1], "40");
        const old = light.inFlightEpoch;
        light.setBrightness(.7);
        verify(!light.accept(JSON.stringify({
            current: 40,
            maximum: 100
        }), old));
        compare(light.brightness, .7);
        light.writer.running = false;
        light.writer.exited(0, 0);
        wait(80);
        compare(light.writer.starts, 2);
        compare(light.writer.command[light.writer.command.length - 1], "70");
        light.present = false;
        verify(!light.setBrightness(.8));
        compare(light.pending, -1);
        verify(!light.accept(JSON.stringify({
            current: 70,
            maximum: 100
        }), old));
    }
    function test_panel_minimum_keyboard_zero_and_invalid_values() {
        const panel = createTemporaryObject(service, test, {
            descriptor: {
                kind: "backlight",
                device: "fixture",
                maximum: 100,
                current: 50
            }
        });
        panel.setBrightness(0);
        compare(panel.brightness, .01);
        verify(!panel.setBrightness(NaN));
        const keys = createTemporaryObject(service, test, {
            descriptor: {
                kind: "keyboard",
                device: "fixture::kbd_backlight",
                maximum: 3,
                current: 2
            }
        });
        keys.setBrightness(0);
        compare(keys.brightness, 0);
        verify(keys.available);
        const missing = createTemporaryObject(service, test);
        verify(!missing.available);
        verify(!missing.setBrightness(.5));
    }
}
