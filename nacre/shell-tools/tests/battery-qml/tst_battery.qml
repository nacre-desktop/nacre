import QtQuick
import QtTest
import "fixtures"

TestCase {
    id: test
    name: "BatteryAlerts"
    when: windowShown
    Component {
        id: owner
        NacreBatteryAlerts {}
    }
    Component {
        id: policy
        NacreBatteryAlertPolicy {}
    }
    function init() {
        const d = UPower.displayDevice;
        d.ready = true;
        d.isLaptopBattery = true;
        d.isPresent = true;
        d.state = UPowerDeviceState.Discharging;
        d.percentage = 1;
        UPower.onBattery = true;
    }
    function setup(level = 0) {
        const view = createTemporaryObject(owner, test);
        verify(view.restore(JSON.stringify({
            version: 1,
            severity: level
        })));
        return view;
    }
    function part(view, name) {
        return findChild(view, "batteryAlert" + name);
    }
    function finish(view, code = 0) {
        const p = part(view, "Sender");
        p.running = false;
        p.exited(code, 0);
    }
    function test_thresholds_and_monotonic_rearm() {
        const p = createTemporaryObject(policy, test);
        verify(p.initialize(0));
        const sample = {
            ready: true,
            present: true,
            discharging: true,
            fraction: 0.21
        };
        compare(p.request(sample), null);
        sample.fraction = 0.20;
        let request = p.request(sample);
        compare(request.level, 1);
        verify(p.delivered(request));
        compare(p.request(sample), null);
        sample.fraction = 0.15;
        request = p.request(sample);
        compare(request.level, 2);
        verify(p.delivered(request));
        compare(p.severity, 2);
        sample.fraction = 0.19;
        compare(p.request(sample), null);
        sample.discharging = false;
        compare(p.request(sample), null);
        compare(p.severity, 0);
        verify(!p.delivered(request)); // reply from the prior discharge cycle
        sample.discharging = true;
        compare(p.request(sample).level, 1);
        sample.fraction = 0.21;
        compare(p.request(sample), null);
    }
    function test_unknown_no_battery_and_invalid_values() {
        const p = createTemporaryObject(policy, test);
        verify(!p.initialize(3));
        compare(p.request({
            ready: true,
            present: true,
            discharging: true,
            fraction: 0
        }), null);
        p.initialize(2);
        const sample = {
            ready: false,
            present: true,
            discharging: true,
            fraction: 0
        };
        compare(p.request(sample), null);
        compare(p.severity, 2);
        sample.ready = true;
        for (const value of [NaN, Infinity, -0.1, 1.1, "0.1"]) {
            sample.fraction = value;
            compare(p.request(sample), null);
            compare(p.severity, 2);
        }
        sample.present = false;
        compare(p.request(sample), null);
        compare(p.severity, 0);
        sample.present = true;
        sample.fraction = 0;
        compare(p.request(sample).level, 2);
    }
    function test_boot_readiness_no_false_zero_and_idle() {
        UPower.displayDevice.ready = false;
        UPower.displayDevice.percentage = 0;
        UPower.displayDevice.state = UPowerDeviceState.Unknown;
        const view = setup();
        wait(200);
        compare(part(view, "Sender").running, false);
        UPower.displayDevice.ready = true;
        wait(200);
        compare(part(view, "Sender").running, false);
        UPower.displayDevice.percentage = 0.75;
        UPower.displayDevice.state = UPowerDeviceState.Discharging;
        wait(200);
        compare(part(view, "Sender").running, false);
        compare(part(view, "Settle").running, false);
        compare(part(view, "Retry").running, false);
        compare(part(view, "Watchdog").running, false);
        compare(part(view, "Ledger").writes, 0);
    }
    function test_delivery_ack_cache_and_restart() {
        UPower.displayDevice.percentage = 0.20;
        const view = setup();
        tryCompare(part(view, "Sender"), "running", true, 500);
        compare(part(view, "Sender").command, ["notify-send", "--app-name", "Battery", "--urgency", "normal", "Battery Low", "Remaining: 20%"]);
        compare(part(view, "Policy").severity, 0);
        finish(view);
        compare(part(view, "Policy").severity, 1);
        const data = part(view, "Ledger").text();
        compare(JSON.parse(data).severity, 1);
        wait(200);
        compare(part(view, "Sender").running, false);
        const restart = createTemporaryObject(owner, test);
        verify(restart.restore(data));
        wait(200);
        compare(part(restart, "Sender").running, false);
        UPower.displayDevice.percentage = 0.15;
        tryCompare(part(view, "Sender"), "running", true, 500);
        compare(part(view, "Sender").command[4], "critical");
        finish(view);
        compare(part(view, "Policy").severity, 2);
        UPower.displayDevice.percentage = 0.19;
        wait(200);
        compare(part(view, "Sender").running, false);
    }
    function test_charge_invalid_cache_failure_retry_and_timeout() {
        const view = createTemporaryObject(owner, test);
        verify(!view.restore("broken"));
        verify(view.cacheError.length > 0);
        UPower.onBattery = false;
        UPower.displayDevice.percentage = 0.1;
        wait(200);
        compare(part(view, "Sender").running, false);
        UPower.onBattery = true;
        tryCompare(part(view, "Sender"), "running", true, 500);
        finish(view, 1);
        compare(part(view, "Policy").severity, 0);
        verify(view.deliveryError.length > 0);
        verify(part(view, "Retry").running);
        part(view, "Retry").stop();
        view.evaluate();
        compare(part(view, "Sender").running, true);
        part(view, "Watchdog").triggered();
        compare(view.failures, 2);
        compare(part(view, "Policy").severity, 0);
        part(view, "Retry").stop();
        view.evaluate();
        finish(view, 1);
        compare(view.failures, 3);
        compare(part(view, "Retry").running, false);
        view.evaluate();
        finish(view);
        compare(part(view, "Policy").severity, 2);
        part(view, "Ledger").saveFailed();
        verify(view.cacheError.length > 0);
    }
    function test_missing_state_and_late_reply_after_charge() {
        const view = createTemporaryObject(owner, test);
        part(view, "Ledger").loadFailed(FileViewError.FileNotFound);
        verify(view.cacheReady);
        compare(view.cacheError, "");
        UPower.displayDevice.percentage = 0.1;
        tryCompare(part(view, "Sender"), "running", true, 500);
        UPower.onBattery = false;
        view.evaluate();
        finish(view);
        compare(part(view, "Policy").severity, 0);
        wait(200);
        compare(part(view, "Sender").running, false);
        UPower.onBattery = true;
        tryCompare(part(view, "Sender"), "running", true, 500);
        UPower.onBattery = false;
        view.evaluate();
        finish(view, 1);
        compare(view.deliveryError, "");
        compare(part(view, "Retry").running, false);
    }
}
