import QtQuick
import QtTest
import "fixtures"

TestCase {
    id: test
    name: "NacreNotificationLifecycle"
    when: windowShown
    Component {
        id: service
        NacreNotifs {}
    }
    Component {
        id: notice
        QtObject {
            property int noticeId: 1
            property bool tracked: false
            property bool lastGeneration: false
            property bool temporary: false
            property string summary: "Message"
            property string body: "Body"
            property string appName: "Chat"
            property string appIcon: ""
            property string image: ""
            property int urgency: 1
            property real expireTimeout: 20000
            property var actions: []
            property int dismissed: 0
            property int expired: 0
            signal closed(int reason)
            function dismiss() {
                dismissed++;
                closed(2);
            }
            function expire() {
                expired++;
                closed(1);
            }
        }
    }
    function row(key) {
        return {
            key: key,
            time: "2026-10-09T12:00:00.000Z",
            summary: "History",
            body: "Text",
            appName: "Chat",
            appIcon: "",
            image: "",
            urgency: 1
        };
    }
    function init() {
        DesktopSettings.data = {
            dnd: false
        };
        NacrePanelState.screens = {};
        NacreNotifications.defaultExpireTimeout = 5000;
    }
    function test_control_center_history_clock_runs_only_while_visible() {
        const view = createTemporaryObject(service, test);
        NacrePanelState.panels = ({});
        NacrePanelState.screens = ({});
        view.clock = new Date(0);
        NacrePanelState.screens = {
            test: {
                osd: true,
                controlSection: "home"
            }
        };
        verify(view.clock.getTime() > 0);
        verify(findChild(view, "noticeClockTimer").running);
        NacrePanelState.screens = {
            test: {
                osd: true,
                controlSection: "network"
            }
        };
        verify(!findChild(view, "noticeClockTimer").running);
        NacrePanelState.screens = ({});
    }
    function test_clean_load_never_writes_and_live_arrivals_merge_before_save() {
        const state = createTemporaryObject(service, test);
        const writer = findChild(state, "noticeHistoryWriter");
        verify(state.restore(JSON.stringify([row("saved")])));
        wait(150);
        compare(writer.starts, 0);
        compare(state.retained.length, 1);
        const update = createTemporaryObject(notice, test);
        const entry = state.receive(update);
        compare(state.retained.length, 2);
        update.summary = "Updated";
        compare(entry.summary, "Updated");
        compare(state.receive(update), entry);
        compare(state.list.length, 2);
        wait(150);
        compare(writer.starts, 1);
        verify(writer.writes.length > 0);
        const records = JSON.parse(writer.writes[0]);
        compare(records.length, 2);
        verify(!("notification" in records[1]));
        verify(!("actions" in records[1]));
        state.persist();
        state.persist();
        wait(150);
        compare(writer.starts, 1);
        writer.running = false;
        writer.exited(0, 0);
        wait(150);
        compare(writer.starts, 2);
        const waiting = createTemporaryObject(service, test);
        const fresh = createTemporaryObject(notice, test, {
            noticeId: 2
        });
        waiting.receive(fresh);
        verify(!waiting.historyReady);
        verify(waiting.restore(JSON.stringify([row("older")])));
        compare(waiting.retained.length, 2);
    }
    function test_failed_load_protects_file_and_clear_during_load_cannot_resurrect() {
        const state = createTemporaryObject(service, test);
        state.receive(createTemporaryObject(notice, test));
        verify(!state.restore("bad JSON"));
        wait(150);
        compare(findChild(state, "noticeHistoryWriter").starts, 0);
        verify(!state.historyReady);
        state.clearHistory();
        compare(state.retained.length, 0);
        verify(state.restore(JSON.stringify([row("old")])));
        compare(state.retained.length, 0);
    }
    function test_transient_feedback_and_dnd_history_policies() {
        const state = createTemporaryObject(service, test);
        state.restore("[]");
        state.policy = {
            feedbackApps: ["notify-send"],
            feedbackSummaries: ["Screenshot saved"],
            feedbackPatterns: []
        };
        const ephemeral = createTemporaryObject(notice, test, {
            temporary: true
        });
        state.receive(ephemeral);
        compare(state.retained.length, 0);
        state.receive(createTemporaryObject(notice, test, {
            noticeId: 2,
            appName: "notify-send",
            summary: "Screenshot saved"
        }));
        compare(state.retained.length, 0);
        wait(150);
        compare(findChild(state, "noticeHistoryWriter").starts, 0);
        DesktopSettings.data = {
            dnd: true
        };
        state.receive(createTemporaryObject(notice, test, {
            noticeId: 3,
            appName: "Discord",
            summary: "Screenshot saved"
        }));
        compare(state.retained.length, 1);
        compare(state.popups.length, 0);
        DesktopSettings.data = {
            dnd: false
        };
        compare(state.popups.length, 2);
        ephemeral.expire();
        compare(state.retained.length, 1);
    }
    function test_hover_suppression_permanent_timeout_expiry_and_native_close() {
        const state = createTemporaryObject(service, test);
        state.restore("[]");
        const native = createTemporaryObject(notice, test);
        const entry = state.receive(native);
        verify(entry.deadline.running);
        entry.hovered = true;
        verify(!entry.deadline.running);
        NacrePanelState.screens = {
            test: {
                dashboard: true
            }
        };
        verify(!entry.hovered);
        verify(entry.deadline.running);
        NacrePanelState.screens = {};
        verify(entry.deadline.running);
        native.closed(1);
        compare(entry.notification, null);
        verify(!entry.popup);
        verify(!entry.deadline.running);
        compare(state.retained.length, 1);
        compare(entry.body, "Body");
        state.dismiss(entry);
        compare(state.retained.length, 0);
        const permanent = createTemporaryObject(notice, test, {
            noticeId: 2,
            expireTimeout: 0
        });
        const sticky = state.receive(permanent);
        verify(sticky.deadline.running);
        permanent.expireTimeout = 30;
        verify(sticky.deadline.running);
        wait(90);
        compare(permanent.expired, 1);
        compare(state.retained.length, 1);
        verify(!sticky.popup);
    }
}
