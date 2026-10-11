import QtQuick
import QtTest
import "fixtures"

TestCase {
    id: test
    name: "NacreControlTools"
    when: windowShown
    Component {
        id: service
        NacreControlTools {}
    }
    function init() {
        NacrePanelState.screens = ({});
        AppLaunch.calls = [];
    }
    function finish(process, payload) {
        process.stdout.text = JSON.stringify(payload);
        process.stdout.streamFinished();
        process.running = false;
        process.exited(0, 0);
    }
    function test_open_discovery_coalesces_without_device_writes() {
        const view = createTemporaryObject(service, test);
        const process = findChild(view, "controlToolsWorker");
        compare(process.starts, 1);
        compare(process.command[2], "state");
        NacrePanelState.screens = {
            test: {
                osd: true
            }
        };
        compare(process.starts, 1);
        finish(process, {
            nightLightSupported: true,
            nightLightEnabled: false,
            screenshot: true
        });
        tryCompare(process, "starts", 2, 300);
        finish(process, {
            nightLightSupported: true,
            nightLightEnabled: false,
            screenshot: true
        });
        compare(view.data.nightLightEnabled, false);
        compare(AppLaunch.calls.length, 0);
        NacrePanelState.screens = {
            test: {
                osd: false
            }
        };
        wait(400);
        compare(process.starts, 2);
    }
    function test_explicit_night_light_guard_and_action_arguments() {
        const view = createTemporaryObject(service, test);
        const process = findChild(view, "controlToolsWorker");
        finish(process, {
            nightLightSupported: true,
            nightLightExternal: true,
            nightLightEnabled: false
        });
        view.toggleNightLight();
        compare(process.starts, 1);
        view.data = {
            nightLightSupported: true,
            nightLightExternal: false,
            nightLightEnabled: false
        };
        view.toggleNightLight();
        compare(process.command[2], "night-light");
        compare(process.command[3], "on");
        finish(process, {
            error: "Fixture error"
        });
        compare(view.error, "Fixture error");
        compare(view.data.nightLightEnabled, false);
    }
    function test_capture_waits_for_dismissal_and_rejects_unknown_tools() {
        const view = createTemporaryObject(service, test);
        view.capture("shell command");
        wait(350);
        compare(AppLaunch.calls.length, 0);
        view.capture("screenshot");
        compare(NacrePanelState.closes, 1);
        wait(100);
        compare(AppLaunch.calls.length, 0);
        tryVerify(() => AppLaunch.calls.length === 1, 500);
        compare(AppLaunch.calls[0].command[2], "screenshot");
    }
}
