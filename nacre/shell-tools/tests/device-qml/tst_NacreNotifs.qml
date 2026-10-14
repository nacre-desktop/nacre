import QtQuick
import QtTest
import Quickshell.Services.Notifications
import "fixtures"

TestCase {
    id: test
    name: "NacreNotificationLifecycle"
    when: windowShown

    Component {
        id: notice
        NotificationStub {
            property bool tracked: false
            property bool lastGeneration: false
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
        const view = NacreNotifs;
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
        const state = NacreNotifs;
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
    }
    function test_live_arrivals_merge_before_history_load() {
        const waiting = NacreNotifs;
        const fresh = createTemporaryObject(notice, test, {
            noticeId: 2
        });
        waiting.receive(fresh);
        verify(!waiting.historyReady);
        verify(waiting.restore(JSON.stringify([row("older")])));
        compare(waiting.retained.length, 2);
    }
    function test_failed_load_protects_file_and_clear_during_load_cannot_resurrect() {
        const state = NacreNotifs;
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
    function test_bulk_clear_preserves_transient_and_reentrant_arrivals() {
        const state = NacreNotifs;
        verify(state.restore(JSON.stringify([row("saved"), row("older")])));
        const active = createTemporaryObject(notice, test);
        const removed = state.receive(active);
        removed.hovered = true;
        const transient = createTemporaryObject(notice, test, {
            noticeId: 2,
            temporary: true
        });
        const popup = state.receive(transient);
        const arriving = createTemporaryObject(notice, test, {
            noticeId: 3
        });
        active.closed.connect(() => state.receive(arriving));
        state.clearHistory();
        compare(active.dismissed, 1);
        compare(transient.dismissed, 0);
        verify(state.list.includes(popup));
        verify(popup.popup);
        verify(!removed.popup);
        verify(!removed.hovered);
        compare(state.retained.length, 1);
        compare(state.retained[0].notification, arriving);
        compare(Object.keys(state.dismissedKeys).length, 0);
        wait(150);
        const writer = findChild(state, "noticeHistoryWriter");
        compare(writer.starts, 1);
        const records = JSON.parse(writer.writes[0]);
        compare(records.length, 1);
        compare(records[0].key, state.retained[0].key);
    }
    function test_dismissal_tombstones_only_live_until_initial_history_load() {
        const state = NacreNotifs;
        const entry = state.receive(createTemporaryObject(notice, test));
        entry.key = "already-dismissed";
        state.dismiss(entry);
        verify(state.dismissedKeys["already-dismissed"]);
        verify(!state.restore("invalid"));
        verify(state.dismissedKeys["already-dismissed"]);
        verify(state.restore(JSON.stringify([row("already-dismissed"), row("kept")])));
        compare(state.retained.length, 1);
        compare(state.retained[0].key, "kept");
        compare(Object.keys(state.dismissedKeys).length, 0);
        state.clearHistory();
        for (let i = 0; i < 100; i++) {
            const next = state.receive(createTemporaryObject(notice, test, {
                noticeId: i + 10
            }));
            if (i % 2)
                state.dismiss(next);
            else
                state.clearHistory();
        }
        compare(state.list.length, 0);
        compare(Object.keys(state.dismissedKeys).length, 0);
        wait(1);
    }
    function test_closed_and_removed_notifications_drop_images_and_native_references() {
        const state = NacreNotifs;
        verify(state.restore("[]"));
        const live = createTemporaryObject(notice, test, {
            image: "image://qsimage/test",
            expireTimeout: 3000
        });
        const entry = state.receive(live);
        compare(entry.image, live.image);
        state.expireEntry(entry);
        compare(live.expired, 1);
        compare(entry.notification, null);
        compare(entry.image, "");
        compare(entry.snapshot.summary, "Message");
        compare(entry.actions.length, 0);
        verify(!entry.deadline.running);
        const active = createTemporaryObject(notice, test, {
            noticeId: 2,
            image: "file:///demo.png"
        });
        const removed = state.receive(active);
        state.clearHistory();
        compare(active.dismissed, 1);
        compare(removed.notification, null);
        compare(removed.image, "");
        compare(Object.keys(removed.snapshot).length, 0);
        verify(!removed.deadline.running);
        compare(state.list.length, 0);
        wait(1);
    }
    function test_hidden_long_timeout_and_dnd_notifications_still_expire() {
        const state = NacreNotifs;
        verify(state.restore("[]"));
        const live = createTemporaryObject(notice, test, {
            expireTimeout: 20000,
            image: "image://qsimage/long"
        });
        const entry = state.receive(live);
        compare(entry.deadline.interval, 20000);
        compare(entry.popupDeadline.interval, 10000);
        state.hideEntry(entry);
        verify(!entry.popup);
        verify(entry.deadline.running);
        verify(!entry.popupDeadline.running);
        entry.deadline.interval = 30;
        tryCompare(live, "expired", 1);
        compare(entry.notification, null);
        compare(entry.image, "");
        verify(!entry.deadline.running);
        DesktopSettings.data = {
            dnd: true
        };
        const silent = createTemporaryObject(notice, test, {
            noticeId: 2,
            expireTimeout: 30,
            image: "image://qsimage/dnd"
        });
        const history = state.receive(silent);
        verify(!history.popup);
        verify(history.deadline.running);
        tryCompare(silent, "expired", 1);
        compare(history.notification, null);
        compare(history.image, "");
        compare(state.retained.length, 2);
    }
    function test_transient_feedback_and_dnd_history_policies() {
        const state = NacreNotifs;
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
        const state = NacreNotifs;
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
        verify(!state.suppressed);
        verify(entry.hovered);
        verify(!entry.deadline.running);
        NacrePanelState.screens = {
            test: {
                session: true
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
        verify(!sticky.deadline.running);
        verify(sticky.popupDeadline.running);
        permanent.expireTimeout = 30;
        verify(sticky.deadline.running);
        wait(90);
        compare(permanent.expired, 1);
        compare(state.retained.length, 1);
        verify(!sticky.popup);
    }
}
