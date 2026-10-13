import QtQuick
import QtTest
import "fixtures"

TestCase {
    id: test
    name: "NotificationHistoryClear"
    width: 424
    height: 860
    when: windowShown

    Component {
        id: presentation
        Column {
            id: view
            width: 400
            required property var state
            property int created: 0
            Repeater {
                model: view.state.retained
                delegate: NacreNotice {
                    width: 400
                    history: true
                    Component.onCompleted: view.created++
                }
            }
        }
    }
    function test_clear_publishes_once_without_recreating_remaining_cards() {
        const state = NotificationHistory;
        const rows = [];
        for (let i = 0; i < 300; i++)
            rows.push({
                key: "notice-" + i,
                time: "2026-10-13T08:00:00.000Z",
                summary: "Test notification",
                body: "Synthetic history entry",
                appName: "Fixture app",
                appIcon: "",
                image: ""
            });
        verify(state.restore(JSON.stringify(rows)));
        const view = createTemporaryObject(presentation, test, {
            state: state
        });
        compare(view.created, 300);
        const spy = signalSpy.createObject(test, {
            target: state,
            signalName: "listChanged"
        });
        state.clearHistory();
        compare(state.retained.length, 0);
        compare(spy.count, 1);
        compare(view.created, 300);
        // Drain deferred destruction while the production cards still exist.
        wait(1);
        state.clearHistory();
        compare(state.retained.length, 0);
        compare(view.created, 300);
        spy.destroy();
    }
    Component {
        id: signalSpy
        SignalSpy {}
    }
}
