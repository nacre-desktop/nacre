import QtQuick
import QtTest
import "fixtures"

TestCase {
    id: test

    function init() {
        SidebarChat.draft = "";
        SidebarChat.busy = false;
        SidebarChat.sends = 0;
        SidebarChat.stops = 0;
        SidebarChat.question = null;
        SidebarChat.answers = {};
        SidebarChat.messages.clear();
        SidebarChat.replies = 0;
    }

    function test_long_draft_scrolls_and_cursor_stays_visible() {
        const view = createTemporaryObject(pane, test);
        verify(view);
        const input = findChild(view, "sidebarComposer");
        const scroll = findChild(view, "sidebarComposerScroll");
        verify(input);
        verify(scroll);
        input.text = Array(80).fill("Long message line to verify scrolling").join("\n");
        input.forceActiveFocus();
        input.cursorPosition = input.length;
        wait(200);
        verify(scroll.height <= 160);
        verify(scroll.contentHeight > scroll.height);
        verify(scroll.contentItem.contentY > 0);
        const cursor = input.cursorRectangle;
        verify(cursor.y - scroll.contentItem.contentY >= 0);
        verify(cursor.y - scroll.contentItem.contentY + cursor.height <= scroll.height + 2);
        const transcript = findChild(view, "sidebarTranscript");
        verify(transcript.height > 250);
    }

    function test_enter_sends_while_busy_and_shift_enter_adds_newline() {
        const view = createTemporaryObject(pane, test);
        verify(view);
        const input = findChild(view, "sidebarComposer");
        SidebarChat.busy = true;
        input.text = "Follow up now";
        input.forceActiveFocus();
        keyClick(Qt.Key_Return);
        compare(SidebarChat.sends, 1);
        compare(SidebarChat.lastText, "Follow up now");
        compare(input.text, "");
        SidebarChat.draft = "Recovered draft";
        compare(input.text, "Recovered draft");
        input.text = "Two lines";
        keyClick(Qt.Key_Return, Qt.ShiftModifier);
        compare(SidebarChat.sends, 1);
        verify(input.text.indexOf("\n") >= 0);
    }

    function test_question_cards_accept_free_text_and_stay_bounded() {
        const view = createTemporaryObject(pane, test);
        verify(view);
        SidebarChat.question = {
            "id": "card",
            "questions": [
                {
                    "id": "0",
                    "question": "A question with a choice",
                    "options": [
                        {
                            "label": "First"
                        },
                        {
                            "label": "Second"
                        }
                    ]
                },
                {
                    "id": "1",
                    "question": "Another question",
                    "options": []
                }
            ]
        };
        wait(50);
        const questions = findChild(view, "sidebarQuestions");
        const reply = findChild(view, "sidebarReply");
        verify(questions.visible);
        verify(questions.height <= 240);
        verify(!reply.enabled);
        const first = findChild(view, "questionAnswer-0");
        const second = findChild(view, "questionAnswer-1");
        first.text = "Custom choice";
        second.text = "My answer";
        verify(reply.enabled, "answers=" + JSON.stringify(SidebarChat.answers));
        reply.clicked();
        compare(SidebarChat.replies, 1);
        SidebarChat.question = null;
        wait(20);
        compare(questions.height, 0);
    }

    function test_chat_wheel_glides_then_settles_600_pixels_per_notch() {
        for (let i = 0; i < 80; i++)
            SidebarChat.messages.append({
                "id": String(i),
                "role": "assistant",
                "text": "A transcript message to make the list scrollable."
            });
        const view = createTemporaryObject(pane, test);
        verify(view);
        wait(100);
        const transcript = findChild(view, "sidebarTranscript");
        transcript.follow = false;
        transcript.contentY = transcript.originY;
        wait(20);
        const before = transcript.contentY;
        mouseWheel(transcript, 230, 80, 0, -120);
        wait(100);
        const mid = transcript.contentY;
        verify(mid - before > 0 && mid - before < 599);
        wait(230);
        verify(transcript.contentY - mid > 10);
        verify(transcript.contentY - before >= 599, "scroll delta=" + (transcript.contentY - before));
        verify(transcript.contentY - before <= 601);
    }

    function test_hover_layer_follows_round_button_shape() {
        const button = createTemporaryObject(roundButton, test);
        verify(button);
        const layer = findChild(button, "roundHover");
        compare(layer.radius, 40);
        mouseMove(button, 40, 40);
        wait(20);
        verify(layer.hovered);
        compare(layer.radius, button.radius);
        button.radius = 17;
        compare(layer.radius, 17);
    }

    function rows() {
        return Array.from({
            "length": 80
        }, (_, i) => {
            return ({
                    "id": String(i),
                    "role": "assistant",
                    "text": "Message " + i + " with enough text for a scrolling transcript."
                });
        });
    }

    function test_initial_chat_and_history_refresh_stay_at_latest() {
        const data = rows();
        for (const row of data)
            SidebarChat.messages.append(row);
        const view = createTemporaryObject(pane, test);
        wait(150);
        const transcript = findChild(view, "sidebarTranscript");
        verify(transcript.atYEnd);
        SidebarChat.replaceHistory(data.concat([
            {
                "id": "new",
                "role": "assistant",
                "text": "Newest message"
            }
        ]));
        wait(200);
        verify(transcript.atYEnd);
        verify(transcript.follow);
    }

    function test_refresh_preserves_reading_anchor_when_older_messages_are_inserted() {
        const data = rows();
        for (const row of data)
            SidebarChat.messages.append(row);
        const view = createTemporaryObject(pane, test);
        wait(150);
        const transcript = findChild(view, "sidebarTranscript");
        transcript.follow = false;
        transcript.positionViewAtIndex(30, ListView.Beginning);
        wait(50);
        const item = transcript.itemAtIndex(30);
        verify(item);
        const offset = transcript.contentY - item.y;
        SidebarChat.replaceHistory([
            {
                "id": "older",
                "role": "assistant",
                "text": "An older message inserted above"
            }
        ].concat(data));
        wait(200);
        const restored = transcript.itemAtIndex(31);
        verify(restored);
        verify(Math.abs(transcript.contentY - restored.y - offset) < 2);
        verify(!transcript.follow);
    }

    function test_touchpad_continues_after_release_and_stops_at_rest() {
        for (const row of rows())
            SidebarChat.messages.append(row);
        const view = createTemporaryObject(pane, test);
        wait(150);
        const transcript = findChild(view, "sidebarTranscript");
        const wheel = findChild(view, "sidebarWheel");
        transcript.follow = false;
        transcript.contentY = transcript.originY;
        wheel.pixelScroll(50);
        wait(16);
        wheel.pixelScroll(50);
        const released = transcript.contentY;
        wait(150);
        verify(transcript.contentY > released + 10);
        wait(600);
        verify(!transcript.flicking);
    }

    function test_loaded_different_chat_opens_at_bottom() {
        const data = rows();
        for (const row of data)
            SidebarChat.messages.append(row);
        const view = createTemporaryObject(pane, test);
        wait(150);
        const transcript = findChild(view, "sidebarTranscript");
        transcript.follow = false;
        transcript.positionViewAtIndex(30, ListView.Beginning);
        SidebarChat.replaceHistory(data);
        SidebarChat.threadId = "different-thread";
        wait(200);
        verify(transcript.atYEnd);
        verify(transcript.follow);
    }

    function test_scroll_back_to_bottom_resumes_following_streamed_messages() {
        const data = rows();
        for (const row of data)
            SidebarChat.messages.append(row);
        const view = createTemporaryObject(pane, test);
        wait(150);
        const transcript = findChild(view, "sidebarTranscript");
        transcript.follow = false;
        transcript.contentY = transcript.originY + transcript.contentHeight - transcript.height - 100;
        mouseWheel(transcript, 230, 80, 0, -120);
        wait(350);
        verify(transcript.follow);
        SidebarChat.messages.append({
            "id": "incoming",
            "role": "assistant",
            "text": "New output after reaching bottom"
        });
        wait(150);
        verify(transcript.atYEnd);
    }

    function test_focusing_and_sending_does_not_pin_chat() {
        const view = createTemporaryObject(pane, test, {
            "visibilities": {
                "leftPinned": false
            }
        });
        verify(view);
        const input = findChild(view, "sidebarComposer");
        input.forceActiveFocus();
        input.text = "A followup";
        compare(view.visibilities.leftPinned, false);
        keyClick(Qt.Key_Return);
        compare(SidebarChat.sends, 1);
        compare(view.visibilities.leftPinned, false);
    }

    function test_chat_actions_fit_one_row_idle_and_busy() {
        const view = createTemporaryObject(pane, test, {
            "width": 438
        });
        verify(view);
        wait(20);
        const actions = findChild(view, "sidebarActions");
        const send = findChild(view, "sidebarSend");
        const latest = findChild(view, "sidebarLatest");
        compare(send.y, latest.y);
        verify(latest.x + latest.width <= actions.width, "idle action row overflow");
        SidebarChat.busy = true;
        wait(20);
        compare(send.y, latest.y);
        verify(latest.x + latest.width <= actions.width, "busy action row overflow");
    }

    name: "SidebarComposer"
    width: 460
    height: 700
    visible: true
    when: windowShown

    Component {
        id: pane

        ChatPane {
            width: 460
            height: 700
            visibilities: ({
                    "leftPinned": true
                })
        }
    }

    Component {
        id: roundButton

        Rectangle {
            width: 80
            height: 80
            radius: 40
            color: "white"

            NacreInteraction {
                objectName: "roundHover"
            }
        }
    }
}
