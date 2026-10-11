import QtQuick
import QtQuick.Controls
import QtTest
import qs.config
import qs.widgets
import qs.services

TestCase {
    id: test
    name: "NacreFoundation"
    width: 800
    height: 500
    visible: true
    when: windowShown

    function init() {
        NacreTokens.reduceMotion = Qt.binding(() => DesktopSettings.data.reduceMotion === true);
        DesktopSettings.data = {
            animations: true
        };
    }
    function cleanup() {
        NacreTokens.reduceMotion = Qt.binding(() => DesktopSettings.data.reduceMotion === true);
        DesktopSettings.data = {
            animations: true
        };
    }
    function test_textfield_edit_selection_validation_and_escape() {
        const field = createTemporaryObject(editor, test);
        compare(field.font.family, NacreTokens.textFamily);
        compare(field.color, NacreTokens.ink);
        compare(field.selectionColor, NacreTokens.accent);
        field.forceActiveFocus();
        keyClick(Qt.Key_A);
        keyClick(Qt.Key_B);
        compare(field.text, "ab");
        field.selectAll();
        compare(field.selectedText, "ab");
        keyClick(Qt.Key_C);
        compare(field.text, "c");
        field.readOnly = true;
        keyClick(Qt.Key_D);
        compare(field.text, "c");
        field.readOnly = false;
        field.text = "";
        field.validator = numeric;
        keyClick(Qt.Key_A);
        compare(field.text, "");
        keyClick(Qt.Key_5);
        verify(field.acceptableInput);
        keyClick(Qt.Key_Return);
        compare(field.acceptedCount, 1);
        keyClick(Qt.Key_Escape);
        compare(field.escapeCount, 1);
    }
    function test_slider_pointer_keys_clamp_and_external_updates() {
        const slider = createTemporaryObject(levelControl, test);
        compare(slider.orientation, Qt.Vertical);
        slider.value = 0.5;
        compare(slider.moveCount, 0);
        slider.forceActiveFocus();
        keyClick(Qt.Key_Up);
        compare(slider.value, 0.6);
        compare(slider.moveCount, 1);
        mouseClick(slider, 15, 10);
        verify(slider.value >= 0.9);
        verify(slider.moveCount >= 2);
        slider.value = 2;
        compare(slider.value, 1);
        slider.enabled = false;
        const count = slider.moveCount;
        mouseClick(slider, 15, 115);
        compare(slider.value, 1);
        compare(slider.moveCount, count);
    }
    function test_attached_scrollbar_tracks_content_without_filling_track() {
        const list = createTemporaryObject(scrollingList, test);
        const bar = list.ScrollBar.vertical;
        verify(bar.visualSize < 1);
        compare(bar.orientation, Qt.Vertical);
        list.contentY = 200;
        tryCompare(bar, "position", 0.2);
        bar.position = 0.4;
        tryCompare(list, "contentY", 400);
        list.contentHeight = 100;
        tryCompare(bar, "size", 1);
        compare(bar.contentItem.opacity, 0);
    }
    IntValidator {
        id: numeric
        bottom: 0
        top: 9
    }
    Component {
        id: editor
        NacreTextField {
            width: 220
            height: 40
            property int acceptedCount: 0
            property int escapeCount: 0
            onAccepted: acceptedCount++
            Keys.onEscapePressed: escapeCount++
        }
    }
    Component {
        id: levelControl
        NacreSlider {
            width: 30
            height: 120
            stepSize: 0.1
            property int moveCount: 0
            onMoved: moveCount++
        }
    }
    Component {
        id: scrollingList
        Flickable {
            width: 100
            height: 100
            contentHeight: 1000
            ScrollBar.vertical: NacreScrollBar {}
        }
    }
    function test_config_chrome_and_motion_follow_desktop_preferences() {
        compare(NacreFrame.colour, NacreTokens.body);
        compare(NacreFrame.headerHeight, 50);
        DesktopSettings.data = {
            topEdge: false,
            leftEdge: false,
            frameWidth: 14,
            frameRounding: 19,
            reduceMotion: true
        };
        compare(NacreFrame.headerHeight, 0);
        compare(NacreFrame.left, 0);
        compare(NacreFrame.right, 14);
        compare(NacreFrame.rounding, 19);
        compare(NacreAppearance.anim.durations.normal, 0);
        compare(NacreAppearance.font.family.sans, NacreTokens.textFamily);
        compare(NacreLauncher.maxShown, 8);
        compare(NacreDashboard.sizes.mediaCoverArtSize, 150);
        compare(NacreNotifications.defaultExpireTimeout, 5000);
    }
    function test_native_image_resize_original_clear_and_error() {
        const image = createTemporaryObject(picture, test, {
            path: Qt.resolvedUrl("test.png").toString()
        });
        tryCompare(image, "status", Image.Ready);
        image.width = 72;
        image.height = 48;
        image.width = 80;
        wait(100);
        compare(image.decodeSize.width, 80);
        compare(image.decodeSize.height, 48);
        image.loadOriginal = true;
        wait(100);
        compare(image.decodeSize.width, -1);
        image.path = "";
        wait(100);
        compare(String(image.source), "");
        ignoreWarning(/.*Cannot open:.*missing-image\.png/);
        image.path = Qt.resolvedUrl("missing-image.png").toString();
        tryVerify(() => image.error.length > 0);
        compare(String(image.source), "");
        image.path = Qt.resolvedUrl("test.png").toString();
        tryCompare(image, "status", Image.Ready);
        compare(image.error, "");
    }
    Component {
        id: picture
        NacreImage {
            width: 60
            height: 40
        }
    }
    function test_transparent_surface_and_explicit_geometry() {
        const view = createTemporaryObject(surface, test);
        compare(view.color.a, 0);
        compare(view.radius, 0);
        view.animateColor = false;
        view.color = "#123456";
        view.radius = 20;
        view.border.width = 2;
        view.border.color = "white";
        compare(String(view.color), "#123456");
        compare(view.radius, 20);
        compare(view.width, 120);
        compare(view.border.width, 2);
    }
    function test_text_metrics_theme_and_consumer_overrides() {
        const view = createTemporaryObject(label, test);
        compare(view.font.family, "IBM Plex Sans");
        compare(view.font.pointSize, 12);
        compare(view.textFormat, Text.PlainText);
        compare(view.renderType, Text.NativeRendering);
        verify(view.implicitWidth > 0);
        verify(view.implicitHeight > 0);
        view.width = 65;
        view.text = "Several words wrap naturally";
        view.wrapMode = Text.Wrap;
        view.forceLayout();
        verify(view.lineCount > 1);
        view.font.pointSize = 18;
        compare(view.font.pointSize, 18);
        const prior = NacreColours.palette;
        NacreColours.palette = Object.assign({}, prior, {
            m3onSurface: "#d4eac7"
        });
        compare(String(view.color), "#d4eac7");
        NacreColours.palette = prior;
        view.wrapMode = Text.NoWrap;
        view.elide = Text.ElideRight;
        view.forceLayout();
        verify(view.truncated);
    }
    function test_text_motion_settles_after_rapid_hidden_or_reduced_changes() {
        const view = createTemporaryObject(label, test, {
            animate: true,
            animateDuration: 70
        });
        compare(view.scale, 1);
        view.text = "Next";
        verify(view.textTransitionRunning);
        view.text = "Newest";
        wait(110);
        verify(!view.textTransitionRunning);
        compare(view.scale, 1);
        view.text = "Reduce";
        NacreTokens.reduceMotion = true;
        verify(!view.textTransitionRunning);
        compare(view.scale, 1);
        view.text = "No motion";
        verify(!view.textTransitionRunning);
        NacreTokens.reduceMotion = false;
        view.text = "Hide";
        view.visible = false;
        verify(!view.textTransitionRunning);
        compare(view.scale, 1);
        view.text = "Hidden update";
        verify(!view.textTransitionRunning);
    }
    function test_surface_color_motion_stops_with_motion_preference() {
        const view = createTemporaryObject(surface, test, {
            color: "#123456",
            transitionDuration: 100
        });
        view.color = "#aabbcc";
        wait(160);
        compare(String(view.color), "#aabbcc");
        view.color = "#ff0000";
        NacreTokens.reduceMotion = true;
        tryCompare(view, "color", Qt.color("#ff0000"));
        view.color = "#00ff00";
        compare(String(view.color), "#00ff00");
    }
    function test_round_hit_regions_hover_and_one_activation() {
        const view = createTemporaryObject(control, test);
        verify(!view.containsPoint(Qt.point(1, 1)));
        verify(view.containsPoint(Qt.point(50, 30)));
        mouseMove(view, 50, 30);
        tryCompare(view, "hovered", true);
        mouseClick(view, 1, 1);
        compare(view.clicks, 0);
        mouseClick(view, 50, 30);
        compare(view.clicks, 1);
        verify(!view.pressed);
        verify(!view.keyboardFocusVisible);
        mouseMove(test, 500, 450);
        tryCompare(view, "hovered", false);
    }
    function test_disabled_and_hidden_controls_never_activate() {
        const view = createTemporaryObject(control, test);
        view.disabled = true;
        verify(!view.activeFocusOnTab);
        mouseClick(view, 50, 30);
        compare(view.clicks, 0);
        view.activate(null);
        compare(view.clicks, 0);
        verify(!view.hovered);
        view.disabled = false;
        view.enabled = false;
        view.activate(null);
        compare(view.clicks, 0);
        view.enabled = true;
        view.visible = false;
        view.activate(null);
        compare(view.clicks, 0);
    }
    function test_keyboard_focus_activation_and_escape_propagation() {
        const view = createTemporaryObject(control, test);
        view.forceActiveFocus(Qt.TabFocusReason);
        tryCompare(view, "activeFocus", true);
        verify(view.keyboardFocusVisible);
        const ring = findChild(view, "nacreInteractionFocus");
        verify(ring.visible);
        compare(ring.radius, view.radius);
        keyClick(Qt.Key_Return);
        compare(view.clicks, 1);
        keyPress(Qt.Key_Space);
        verify(view.pressed);
        compare(view.clicks, 1);
        keyRelease(Qt.Key_Space);
        compare(view.clicks, 2);
        verify(!view.pressed);
        keyClick(Qt.Key_Escape);
        compare(test.escapes, 1);
        keyPress(Qt.Key_Space);
        view.disabled = true;
        keyRelease(Qt.Key_Space);
        compare(view.clicks, 2);
    }
    function test_pending_feedback_is_safe_when_control_is_destroyed() {
        const view = control.createObject(test);
        verify(view);
        mouseMove(view, 50, 30);
        view.visible = false;
        view.destroy();
        NacreTokens.reduceMotion = true;
        wait(30);
    }

    function test_production_action_button_uses_new_interaction() {
        const view = createTemporaryObject(button, test);
        verify(view.implicitWidth > 60);
        compare(view.implicitHeight, 34);
        compare(view.radius, 17);
        const interaction = view.children.find(child => child.activated !== undefined);
        verify(interaction);
        compare(interaction.Accessible.name, "Action");
        mouseClick(view, view.width / 2, 17);
        compare(view.clicks, 1);
        view.enabled = false;
        mouseClick(view, view.width / 2, 17);
        compare(view.clicks, 1);
    }

    property int escapes: 0
    Keys.onEscapePressed: escapes += 1
    Component {
        id: surface
        NacreSurface {
            width: 120
            height: 60
        }
    }
    Component {
        id: label
        NacreText {
            text: "Foundation"
        }
    }
    Component {
        id: control
        NacreInteraction {
            anchors.fill: undefined
            width: 100
            height: 60
            radius: 30
            property int clicks: 0
            function onClicked(event) {
                clicks += 1;
            }
        }
    }
    Component {
        id: button
        ActionButton {
            text: "Action"
            width: implicitWidth
            height: implicitHeight
            property int clicks: 0
            onClicked: clicks += 1
        }
    }
}
