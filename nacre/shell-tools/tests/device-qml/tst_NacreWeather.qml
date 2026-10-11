import QtQuick
import QtTest
import "fixtures"

TestCase {
    id: test
    name: "NacreWeather"
    when: windowShown
    Component {
        id: service
        NacreWeather {}
    }
    function cleanup() {
        DesktopSettings.data = ({});
    }
    function reading() {
        return {
            location: "Fixture city",
            code: "113",
            description: "Sunny",
            temperature: 20,
            feelsLike: 19,
            high: 25,
            low: 14,
            checked: Date.now() / 1000,
            stale: false,
            error: ""
        };
    }
    function test_unknown_atomic_validation_and_units() {
        const view = createTemporaryObject(service, test);
        verify(!Number.isFinite(view.temperature));
        compare(view.displayTemperature, "");
        verify(view.publish(JSON.stringify(reading()), false));
        compare(view.icon, "clear_day");
        compare(view.displayTemperature, "20°C");
        compare(view.range, "High 25°C · Low 14°C");
        DesktopSettings.data = {
            weatherFahrenheit: true
        };
        compare(view.displayTemperature, "68°F");
        compare(view.detail, "Feels like 66°F");
        const previous = view.reading;
        verify(!view.publish('{"temperature":999}', false));
        compare(view.reading, previous);
        verify(view.stale);
        verify(view.error.length > 0);
        verify(view.displayTemperature.includes("cached"));
        const older = reading();
        older.checked -= 3600;
        verify(!view.publish(JSON.stringify(older), true));
        compare(view.reading, previous);
    }
    function test_failure_retains_timestamp_and_no_data_is_unknown() {
        const view = createTemporaryObject(service, test);
        verify(!view.publish('{"error":"Offline"}', false));
        verify(!Number.isFinite(view.temperature));
        verify(view.publish(JSON.stringify(reading()), false));
        const checked = view.reading.checked;
        const cached = Object.assign({}, view.reading, {
            stale: true,
            error: "Offline"
        });
        verify(view.publish(JSON.stringify(cached), false));
        compare(view.reading.checked, checked);
        verify(view.stale);
        compare(view.error, "Offline");
    }
    function test_coalescing_and_old_city_result_rejected() {
        const view = createTemporaryObject(service, test);
        const reader = findChild(view, "weatherReader");
        verify(reader.running);
        view.reload();
        view.reload();
        verify(view.queued);
        DesktopSettings.data = {
            weatherLocation: "New city"
        };
        reader.stdout.text = JSON.stringify(reading());
        reader.stdout.streamFinished();
        verify(!Number.isFinite(view.temperature));
        reader.running = false;
        reader.exited(0, 0);
        tryCompare(reader, "starts", 2, 300);
        compare(view.requestLocation, "New city");
        const data = reading();
        data.location = "New city";
        reader.stdout.text = JSON.stringify(data);
        reader.stdout.streamFinished();
        reader.running = false;
        reader.exited(0, 0);
        compare(view.location, "New city");
        wait(150);
        compare(reader.starts, 2);
    }
}
