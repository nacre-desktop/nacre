import QtQuick
import QtTest
import "fixtures"

TestCase {
    id: test
    name: "NacreCivilClock"
    when: windowShown
    Component {
        id: service
        NacreTime {}
    }
    function test_shared_civil_date_minute_default_seconds_opt_in_and_enable() {
        const clock = createTemporaryObject(service, test);
        compare(clock.format("yyyy-MM-dd HH:mm"), "2026-10-09 12:34");
        compare(clock.hours, 12);
        compare(clock.minutes, 34);
        compare(clock.seconds, 0);
        clock.secondsEnabled = true;
        compare(clock.seconds, 56);
        clock.enabled = false;
        verify(!findChild(clock, "nacreSystemClock").enabled);
        clock.enabled = true;
        verify(clock.enabled);
    }
}
