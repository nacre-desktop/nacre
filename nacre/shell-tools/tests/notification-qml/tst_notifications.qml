import QtQuick
import QtTest
import "fixtures"

TestCase {
    id: test
    name: "NacreNotifications"
    width: 600
    height: 700
    visible: true
    when: windowShown
    Component {
        id: entry
        QtObject {
            property string key: "fixture"
            property string appName: "Fixture app"
            property string appIcon: ""
            property string summary: "A message arrived"
            property string body: "Body text"
            property string image: ""
            property string timeStr: "Now"
            property int urgency: 1
            property bool hovered: false
            property var notification: ({})
            property int invoked: 0
            property var actions: []
        }
    }
    Component {
        id: card
        NacreNotice {
            width: 400
        }
    }
    Component {
        id: stack
        NacreNotificationStack {}
    }
    function init() {
        NacreNotifs.popups = [];
        NacreNotifs.dismissed = "";
        NacreNotifications.actionOnClick = false;
    }
    function makeCard() {
        const data = createTemporaryObject(entry, test);
        return createTemporaryObject(card, test, {
            modelData: data
        });
    }
    function test_text_bounds_expand_and_plain_text() {
        const view = makeCard();
        view.modelData.summary = "A long summary with words ".repeat(20);
        view.modelData.body = "<b>Not executable markup</b> and a longer message ".repeat(30);
        wait(30);
        const compact = view.height;
        const body = findChild(view, "noticeBody");
        compare(body.textFormat, Text.PlainText);
        mouseClick(findChild(view, "noticeExpand"), 15, 15);
        wait(30);
        verify(view.expanded);
        verify(view.height > compact);
        verify(body.y + body.height <= view.height - 14);
        verify(body.width <= view.width - 28);
        const controls = findChild(view, "noticeActions");
        verify(!controls.visible);
        mouseClick(findChild(view, "noticeExpand"), 15, 15);
        compare(view.expanded, false);
    }
    function test_close_does_not_expand_and_right_click_dismisses() {
        const view = makeCard();
        mouseClick(findChild(view, "noticeDismiss"), 15, 15);
        compare(NacreNotifs.dismissed, "fixture");
        compare(view.expanded, false);
        NacreNotifs.dismissed = "";
        mouseClick(view, 100, 65, Qt.RightButton);
        compare(NacreNotifs.dismissed, "fixture");
    }
    function test_default_action_is_explicit_and_frozen_history_has_none() {
        const view = makeCard();
        const data = view.modelData;
        data.actions = [
            {
                identifier: "reply",
                text: "Reply",
                invoke: () => data.invoked += 10
            }
        ];
        NacreNotifications.actionOnClick = true;
        view.openDetails();
        compare(data.invoked, 0);
        compare(view.expanded, true);
        data.actions = [
            {
                identifier: "default",
                text: "Open",
                invoke: () => data.invoked++
            },
            {
                identifier: "reply",
                text: "Reply",
                invoke: () => data.invoked += 10
            }
        ];
        view.openDetails();
        compare(data.invoked, 1);
        compare(view.availableActions.length, 1);
        data.notification = null;
        compare(view.availableActions.length, 0);
        view.openDetails();
        compare(data.invoked, 1);
        data.notification = ({});
        view.history = true;
        compare(view.availableActions.length, 0);
        view.openDetails();
        compare(data.invoked, 1);
    }
    function test_keyboard_and_drag_dismissal() {
        const view = makeCard();
        view.forceActiveFocus();
        keyClick(Qt.Key_Space);
        compare(view.expanded, true);
        keyClick(Qt.Key_Delete);
        compare(NacreNotifs.dismissed, "fixture");
        NacreNotifs.dismissed = "";
        mousePress(view, 70, 70);
        mouseMove(view, 105, 70);
        mouseRelease(view, 105, 70);
        wait(220);
        compare(view.dragOffset, 0);
        compare(NacreNotifs.dismissed, "");
        mousePress(view, 70, 70);
        mouseMove(view, 270, 70);
        mouseRelease(view, 270, 70);
        compare(NacreNotifs.dismissed, "fixture");
    }
    function test_burst_cap_suppression_and_hover_release() {
        const data = createTemporaryObject(entry, test);
        NacreNotifs.popups = Array.from({
            length: 12
        }, () => data);
        const view = createTemporaryObject(stack, test);
        wait(40);
        verify(view.height > 0);
        verify(view.height <= test.height - 24);
        const stream = findChild(view, "notificationStream");
        verify(stream.contentHeight > stream.height);
        data.hovered = true;
        view.suppressed = true;
        compare(view.visible, false);
        compare(view.height, 0);
        compare(data.hovered, false);
        compare(NacreNotifs.popups.length, 12);
        view.suppressed = false;
        wait(30);
        verify(view.visible);
        NacreNotifs.popups = [];
        tryCompare(view, "visible", false, 500);
    }
}
