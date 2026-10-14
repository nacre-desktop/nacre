import QtQuick
import QtTest
import "fixtures"

TestCase {
    id: test
    name: "NacreBarControls"
    width: 700
    height: 600
    visible: true
    when: windowShown
    function init() {
        NacreCompositor.activeWindow = null;
        NacreCompositor.overviewHoverWindow = null;
        NacreCompositor.windows = [];
        NacreCompositor.backendName = "hyprland";
        NacreCompositor.activeWorkspaceId = null;
        ChatWindowTitle.titles = ({});
        NacreAudio.volume = .5;
        NacreAudio.writes = [];
        NacreAudio.muted = false;
        NacreNetwork.active = null;
        NacreBluetooth.powered = false;
        NacreNotifs.retained = [];
        UPower.onBattery = true;
        UPower.displayDevice = {
            ready: true,
            isLaptopBattery: true,
            percentage: .53,
            timeToEmpty: 5400,
            timeToFull: 1800
        };
        PowerProfiles.hasPerformanceProfile = true;
        PowerProfiles.profile = PowerProfile.Balanced;
        PowerProfiles.writes = 0;
        NacrePanelState.view = {
            session: false,
            launcher: true,
            dashboard: true,
            osd: true
        };
        NacrePanelState.panels = {
            test: {
                popouts: {
                    hasCurrent: true,
                    pinned: true
                }
            }
        };
        NacreTime.date = new Date(2026, 9, 9, 12, 34);
    }
    Component {
        id: title
        NacreActiveTitle {
            monitor: null
            width: 200
            height: 30
            horizontal: true
        }
    }
    Component {
        id: status
        NacreStatusIcons {
            horizontal: true
            rotation: -90
        }
    }
    Component {
        id: battery
        NacreBatteryPopup {}
    }
    Component {
        id: calendar
        NacreCalendarPopup {}
    }
    Component {
        id: power
        NacrePowerButton {}
    }
    function test_title_plain_elided_native_removed_and_bounded_wheel() {
        const view = createTemporaryObject(title, test);
        compare(view.displayTitle, "Desktop");
        NacreCompositor.activeWindow = {
            wmClass: "editor",
            title: "<b>Long title</b> " + "abc".repeat(100)
        };
        compare(view.displayTitle, NacreCompositor.activeWindow.title);
        const caption = findChild(view, "nacreActiveTitle");
        verify(caption.width < view.width);
        compare(caption.textFormat, Text.PlainText);
        compare(caption.elide, Text.ElideRight);
        NacreCompositor.activeWindow = null;
        compare(view.displayTitle, "Desktop");
        view.adjustVolume(120);
        compare(NacreAudio.volume, .55);
        NacreAudio.volume = .99;
        view.adjustVolume(120);
        compare(NacreAudio.volume, 1);
        NacreAudio.volume = .01;
        view.adjustVolume(-120);
        compare(NacreAudio.volume, 0);
        const writes = NacreAudio.writes.length;
        view.adjustVolume(NaN);
        compare(NacreAudio.writes.length, writes);
    }
    function test_named_chat_preserves_spinner_without_directory_suffix() {
        const view = createTemporaryObject(title, test);
        NacreCompositor.activeWindow = {
            windowId: "niri-window:7",
            workspaceId: "niri-workspace:2",
            wmClass: "nacre-ai-task",
            pid: 123,
            title: "⠹ Fix the terminal titles | 7ce71273f1db4999bf221..."
        };
        compare(view.displayTitle, "⠹ Fix the terminal titles");
        ChatWindowTitle.titles = ({
                123: "Name saved by the agent"
            });
        compare(view.displayTitle, "⠹ Name saved by the agent");
        const caption = findChild(view, "nacreActiveTitle");
        const spinner = findChild(view, "nacreActiveTitleSpinner");
        compare(caption.text, "Name saved by the agent");
        const spinnerY = spinner.mapToItem(view, 0, 0).y;
        const titleX = caption.mapToItem(view, 0, 0).x;
        for (const frame of "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏") {
            NacreCompositor.activeWindow = Object.assign({}, NacreCompositor.activeWindow, {
                title: frame + " Fix the terminal titles | 7ce71273f1db4999bf221..."
            });
            compare(spinner.text, frame);
            compare(spinner.mapToItem(view, 0, 0).y, spinnerY);
            compare(caption.mapToItem(view, 0, 0).x, titleX);
        }
        NacreCompositor.activeWindow = Object.assign({}, NacreCompositor.activeWindow, {
            title: "Name saved by the agent | /home/user/project"
        });
        compare(view.displayTitle, "Name saved by the agent");
        compare(caption.text, "Name saved by the agent");
        verify(!spinner.parent.visible);
    }
    function test_screenshot_focus_loss_keeps_caption_without_stale_empty_workspace_title() {
        NacreCompositor.backendName = "niri";
        NacreCompositor.activeWorkspaceId = "niri-workspace:2";
        const client = {
            windowId: "niri-window:7",
            workspaceId: "niri-workspace:2",
            wmClass: "kitty",
            title: "Terminal"
        };
        NacreCompositor.windows = [client];
        NacreCompositor.activeWindow = client;
        const view = createTemporaryObject(title, test);
        compare(view.displayTitle, "Terminal");
        NacreCompositor.activeWindow = null;
        compare(view.displayTitle, "Terminal");
        NacreCompositor.activeWorkspaceId = "niri-workspace:3";
        compare(view.displayTitle, "Desktop");
        NacreCompositor.activeWorkspaceId = "niri-workspace:2";
        compare(view.displayTitle, "Terminal");
        NacreCompositor.windows = [];
        compare(view.displayTitle, "Desktop");
    }
    function test_overview_hover_caption_changes_without_changing_active_window() {
        const active = {
            windowId: "one",
            wmClass: "editor",
            title: "Focused editor"
        };
        NacreCompositor.activeWindow = active;
        const view = createTemporaryObject(title, test);
        compare(view.displayTitle, "Focused editor");
        NacreCompositor.overviewHoverWindow = {
            windowId: "two",
            wmClass: "nacre-ai-task",
            pid: 123,
            title: "⠹ Other chat | 7ce71273f1db4999bf221..."
        };
        compare(view.displayTitle, "⠹ Other chat");
        compare(NacreCompositor.activeWindow, active);
        NacreCompositor.overviewHoverWindow = null;
        compare(view.displayTitle, "Focused editor");
    }
    function test_short_title_painted_group_is_centered_and_resizes() {
        const view = createTemporaryObject(title, test, {
            width: 650
        });
        for (const text of ["Files", "Terminal", "Desktop"]) {
            NacreCompositor.activeWindow = {
                wmClass: "editor",
                title: text
            };
            wait(0);
            const caption = findChild(view, "nacreActiveTitle");
            const row = caption.parent;
            const glyph = row.children[0];
            glyph.font.family = "Nacre deliberately absent icon font";
            wait(40);
            const left = glyph.mapToItem(view, 0, 0).x;
            const right = caption.mapToItem(view, 0, 0).x + caption.contentWidth;
            verify(Math.abs((left + right) / 2 - view.width / 2) < 1, "painted group center: " + JSON.stringify({
                text: text,
                left: left,
                right: right,
                center: view.width / 2,
                iconWidth: glyph.width,
                iconImplicit: glyph.implicitWidth,
                captionWidth: caption.width,
                captionContent: caption.contentWidth,
                captionImplicit: caption.implicitWidth,
                rowWidth: row.width
            }));
            view.width = 260;
            wait(0);
            verify(row.x >= 0 && row.x + row.width <= view.width + 1);
            view.width = 650;
        }
        NacreCompositor.activeWindow = {
            wmClass: "editor",
            title: "A very long title ".repeat(50)
        };
        wait(0);
        const caption = findChild(view, "nacreActiveTitle");
        verify(caption.parent.width <= view.width);
        compare(NacreAudio.writes.length, 0);
    }
    function test_status_geometry_reactivity_unknown_battery_and_no_writes() {
        const view = createTemporaryObject(status, test);
        compare(view.batteryPercent, 53);
        const handles = [view.audioItem, view.network, view.bluetoothItem, view.battery, view.notificationsItem];
        const points = handles.map(item => item.mapToItem(test, item.width / 2, item.height / 2));
        for (let i = 1; i < points.length; i++)
            verify(points[i].x > points[i - 1].x);
        NacreAudio.muted = true;
        NacreNetwork.active = {
            strength: 80
        };
        NacreBluetooth.powered = true;
        NacreNotifs.retained = [
            {},
            {}
        ];
        compare(view.audioItem.icon, "volume_off");
        compare(view.network.icon, "signal_wifi_4_bar");
        compare(view.bluetoothItem.icon, "bluetooth");
        compare(view.notificationsItem.detail, "2");
        UPower.displayDevice = {
            ready: false,
            isLaptopBattery: true,
            percentage: 0
        };
        verify(!view.batteryAvailable);
        compare(view.batteryPercent, -1);
        compare(view.battery.detail, "");
        compare(NacreAudio.writes.length, 0);
        compare(PowerProfiles.writes, 0);
        verify(view.implicitWidth > 0 && view.implicitHeight > 100 && view.implicitHeight < 350);
        for (const item of handles)
            findChild(item, "nacreStatusGlyph").font.family = "Nacre deliberately absent icon font";
        wait(0);
        verify(view.implicitHeight > 100 && view.implicitHeight < 350);
        compare(findChild(view.audioItem, "nacreStatusGlyph").width, 24);
    }
    function test_battery_validity_estimates_and_explicit_profile_guards() {
        const view = createTemporaryObject(battery, test);
        compare(view.percent, 53);
        compare(view.estimate, "1h 30m remaining");
        compare(PowerProfiles.writes, 0);
        compare(view.formatSeconds(0, "Unknown"), "Unknown");
        compare(view.formatSeconds(NaN, "Unknown"), "Unknown");
        PowerProfiles.hasPerformanceProfile = false;
        verify(!findChild(view, "nacreProfilePerformance").enabled);
        verify(!view.chooseProfile(PowerProfile.Performance));
        verify(!view.chooseProfile(999));
        compare(PowerProfiles.writes, 0);
        verify(view.chooseProfile(PowerProfile.PowerSaver));
        compare(PowerProfiles.profile, PowerProfile.PowerSaver);
        compare(PowerProfiles.writes, 1);
        UPower.onBattery = false;
        compare(view.estimate, "30m until full");
        UPower.displayDevice = {
            ready: true,
            isLaptopBattery: false,
            percentage: .9
        };
        compare(view.percent, -1);
        compare(view.estimate, "Battery unavailable");
        UPower.displayDevice = {
            ready: true,
            isLaptopBattery: true,
            percentage: NaN
        };
        verify(!view.available);
    }
    function test_calendar_civil_rollover_leap_today_and_bad_navigation() {
        const view = createTemporaryObject(calendar, test);
        compare(view.days.length, 42);
        compare(view.todayKey, "2026-10-9");
        view.year = 2026;
        view.month = 11;
        view.changeMonth(1);
        compare(view.year, 2027);
        compare(view.month, 0);
        view.changeMonth(-1);
        compare(view.year, 2026);
        compare(view.month, 11);
        view.year = 2000;
        view.month = 1;
        verify(view.days.some(day => day.inMonth && day.day === 29));
        view.year = 1900;
        verify(!view.days.some(day => day.inMonth && day.day === 29));
        view.firstWeekday = 1;
        compare(new Date(view.year, view.month, 1, 12).getDay(), 4);
        view.changeMonth(NaN);
        compare(view.month, 1);
        view.changeMonth(10001);
        compare(view.month, 1);
        NacreTime.date = new Date(2026, 9, 10, 12);
        compare(view.todayKey, "2026-10-10");
    }
    function test_power_keyboard_only_toggles_menu_not_machine() {
        const view = createTemporaryObject(power, test);
        verify(!NacrePanelState.view.session);
        const action = findChild(view, "nacrePowerActivation");
        action.forceActiveFocus();
        keyClick(Qt.Key_Return);
        verify(!NacrePanelState.view.session);
        compare(NacrePanelState.view.controlSection, "session");
        verify(!NacrePanelState.view.launcher);
        verify(!NacrePanelState.view.dashboard);
        verify(NacrePanelState.view.osd);
        verify(!NacrePanelState.panels.test.popouts.hasCurrent);
        verify(!NacrePanelState.panels.test.popouts.pinned);
        keyClick(Qt.Key_Return);
        verify(!NacrePanelState.view.session);
        verify(!NacrePanelState.view.osd);
        compare(NacreAudio.writes.length, 0);
        compare(PowerProfiles.writes, 0);
    }
}
