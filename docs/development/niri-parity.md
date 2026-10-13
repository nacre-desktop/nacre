# Niri parity audit — 2026-10-13

Keybinding follow-up supersedes the Q-family rows in the original inventory below:

| General binding | Hyprland before → after | Niri before → after |
|---|---|---|
| Super+Q | trash/undo → plain close, works | missing → plain close, works |
| Super+Ctrl+Q | plain close → unassigned | plain close → unassigned |
| Shift+Q / Alt+Q / Ctrl+Shift+Q with Super | personal actions in defaults → opt-in only | unsupported entries → absent from defaults/report |

Both close results use fresh default preparations, full generated bindings, an
actual Super+Q key event, window removal and client exit. The opt-in probe uses
a private headless labwc parent, private buses and disposable HOME directories:

```sh
python tools/tests/verify_plain_close.py --backend hyprland --output /tmp/close-hyprland.json
python tools/tests/verify_plain_close.py --backend niri --output /tmp/close-niri.json
```

It requires labwc, the compositor binaries, Kitty, a C compiler, wayland-scanner
and Wayland/XKB development libraries. `--parent` and `--parent-libraries` accept
an isolated labwc build. The test downloads a pinned, checksum-verified virtual
keyboard protocol; niri receives standard key codes through the private parent,
since direct virtual-keyboard input bypasses its compositor bindings. Hyprland
receives virtual-keyboard input on its private child socket. No live desktop
socket or input device is used. Kitty's test client disables its close prompt.
The `windowTrashBindings` profile opt-in retains the four personal actions on
Hyprland and never overrides Super+Q. Niri still lacks trash/force-kill primitives.

Scope: the shared compositor contract, usage guide, shell IPC handlers and launcher
catalog. Baseline is `0ecb3bb7`; after is this change. **works** means the stated
nested scope passed; **degraded** includes a semantic difference or incomplete
acceptance, and **missing** means an unsupported operation. A degraded test result
is not a claim that the owner's live desktop is broken.

Evidence: **H** = nested Hyprland 0.56.2; **N0** = nested stock niri 26.04 baseline;
**N1** = nested niri candidate; **V** = existing overview companion's native pointer
and actual QML caption verifier; **G** = generated KDL and native validation;
**U** = regression/Qt fixtures. Key rows assess the generated route, not physical
key delivery. Untested external actions remain degraded on both compositors.

All probes use disposable HOME/runtime directories and a private bus without
activation services. They never select the live IPC socket, import its activation
environment, or run UWSM. System bus, PipeWire, accounts and user-manager are
excluded. Niri frame capture passes; Hyprland's nested renderer timed out in grim,
and its forced-X11/pearl/hide/trash probes did not pass. Those are disclosed rather
than treated as successful baseline evidence. Authentication and hardware are
not simulated.

Run the opt-in probe from the source checkout inside an existing Wayland desktop:

```sh
python tools/tests/verify_compositor_parity.py --backend niri --output /tmp/niri-parity.json
python tools/tests/verify_compositor_parity.py --backend hyprland --output /tmp/hyprland-parity.json
```

The JSON is a bounded acceptance record; adjacent logs stay local and may contain
private paths. Do not publish the logs. The probe closes only its own processes.
Its idle and lock tests do not authenticate, suspend, or touch the real session.
For the companion, use `tools/tests/verify_niri_overview_hover.py` with its existing
verified binary and a temporary output path; do not run its installer as a test.

## Feature matrix

| Feature | Hyprland | Niri before | Niri after | Evidence / limit |
|---|---|---|---|---|
| Workspace necklace, focus and moves | works | works | works | N0/N1 window/workspace IPC; dynamic indices on the focused output, stable opaque IDs. |
| Numbered workspace identities | works | degraded | degraded | N0/N1 + U: niri numbers are output-relative display indices; absent targets fail. Startup uses names. |
| Window close / undo trash | degraded | missing | missing | H nested trash did not move the test window; U covers established Hyprland path. Niri has no hidden special workspace; windowTrash=false. |
| Minimize, restore, scratchpad, sticky windows | degraded | missing | missing | H nested hide failed; U covers Hyprland special workspaces. Niri windowHide/scratchpad/pinWindow=false; unavailable bindings omitted. |
| Groups / tabs | degraded | missing | degraded | G + N1 two windows joined into a tabbed column and focus changed. Niri tabs belong to a column; joining columns and drag interaction remain different. |
| Alt+Tab cycle | degraded | missing | works | N0 CLI unavailable; N1 CLI accepted; U proves creation-order wrap within the active workspace, excluding other workspaces. |
| Float, resize, placement, maximize | works | works | works | H/N0/N1 facade actions; niri geometry stays measured in logical pixels. Physical mixed-scale placement remains pending. |
| Searchable overview / live thumbnails | degraded | degraded | degraded | H/N0/N1 searchable overview opens. Niri windowCapture=false: app/title cards work, per-window live previews disabled. |
| Native overview / hover title | degraded | degraded | degraded | N0/N1 native overview; V verifies actual hover captions with companion. Stock niri lacks hover IPC and now explains this instead of showing a misleading hover title. |
| Pearl open, placement, focus, explicit dock | degraded | works | works | N0/N1 real floating pearl and dock IPC; N1 native pearl focus asserted; H nested pearl did not surface. U covers title+PID ownership and bounded handoff. Native modifier drag remains unverified. |
| Launcher, palette, clipboard, frame, controls, OSD | works | works | works | H/N0/N1 panel-state assertions. Real audio/brightness events and clipboard contents require service/hardware acceptance. |
| Screenshots | degraded | degraded | degraded | H grim timed out. N0/N1 grim and native window PNGs pass. Native area picker keys and focused-output save are generated; interactive selection and multi-output capture remain pending. |
| Manual lock / idle / stay awake | degraded | degraded | degraded | H/N1 lock client connects; N1 onLockLocked acknowledgement. Nested hypridle callback failed with isolated system bus. Password, inhibitors and suspend require owner acceptance. |
| Night light | degraded | missing | degraded | U owned wlsunset start/stop and external-controller protection. N1 lacks gamma control, correctly disabled with reason. Physical gamma and logout reset unverified. |
| Startup apps / X11 satellite | degraded | degraded | degraded | U startup is idempotent and niri routes use names. N1 forced-X11 launch/focus/close passes; login activation, input and satellite lifetime need acceptance. |
| Portals / screen sharing | degraded | missing | degraded | Niri portal selection uses GNOME, including diagnostics/repair. GNOME backend is absent in the test environment; end-to-end sharing unverified. Dependency provisioning belongs to separate active work. |
| Blur / window rules | degraded | degraded | degraded | G native KDL accepts bar blur, floating dimensions/corners and rules. Full-output drawers are excluded; shapedBackgroundBlur=false because Nacre has no ext-background-effect shape request. Visual mask acceptance remains pending. |
| Desktop settings / shortcut list | degraded | missing | degraded | N0 settings returned errors; N1 preference/effect saves pass. G/U generated keys can be inspected; bindingIntrospection=false means private overrides are not claimed as current bindings. Outer gaps have no exact shared mapping. |
| Output scale, mode, position / hotplug | degraded | missing | degraded | N1 scale 1→1.25→original passes. U rollback on output disappearance; separate niri layout/pending state. Real modes, two outputs and hotplug unverified; mirroring disabled (monitorMirroring=false). |
| Optional AI/Brain, apps, power, time, network, media, notifications | degraded | degraded | degraded | Shared code remains compositor-independent; U fixtures cover routes. Isolated bus/HOME omit hardware, accounts, user-manager and external services. No live actions were run. |

Niri capability flags cover unsupported sticky windows, hide/trash, scratchpad,
force-kill, pseudotiling, split layout, keymaps, non-consuming/release hooks,
per-window live capture, cursor position and mirroring. Corresponding generated
bindings are omitted and relevant Settings controls are disabled with explanations.
Hover support is negotiated from the companion stream. Bar/floating blur is
ported, but full-output panel blur is excluded until the client can request a
shape through ext-background-effect. No hidden workspace, sticky floating window,
physical key release or current-private-binding list is fabricated.

Native references: [workspace model](https://niri-wm.github.io/niri/Workspaces.html),
[column tabs](https://niri-wm.github.io/niri/Tabs.html),
[output IPC](https://niri-wm.github.io/niri/niri_ipc/enum.OutputAction.html),
[include merge rules](https://niri-wm.github.io/niri/Configuration:-Include.html),
[background effects](https://niri-wm.github.io/niri/Window-Effects.html) and
[screencasting](https://niri-wm.github.io/niri/Screencasting.html).

## Every contract binding

G validates emitted routes; N1 exercises representative facade actions and shell
IPC, not every physical chord. `degraded (G)` means the route parses but its app,
hardware side effect, or input semantics have not been accepted in a real session.
Niri workspace numbers refer only to existing workspaces on the focused output.

| Keys / map | Action | Hyprland | Niri before | Niri after | Evidence / difference |
|---|---|---|---|---|---|
| XF86AudioMute | mute | degraded | degraded | degraded | G; physical key/app or hardware action pending. |
| XF86AudioLowerVolume | volume-down | degraded | degraded | degraded | G; physical key/app or hardware action pending. |
| XF86AudioRaiseVolume | volume-up | degraded | degraded | degraded | G; physical key/app or hardware action pending. |
| XF86AudioMicMute | mic-mute | degraded | degraded | degraded | G; physical key/app or hardware action pending. |
| XF86PowerOff | lock | degraded | degraded | degraded | G; physical key/app or hardware action pending. |
| XF86MonBrightnessDown | brightness-down | degraded | degraded | degraded | G; physical key/app or hardware action pending. |
| XF86MonBrightnessUp | brightness-up | degraded | degraded | degraded | G; physical key/app or hardware action pending. |
| XF86KbdBrightnessUp | keyboard-up | degraded | degraded | degraded | G; physical key/app or hardware action pending. |
| XF86KbdBrightnessDown | keyboard-down | degraded | degraded | degraded | G; physical key/app or hardware action pending. |
| SUPER + period | emoji | degraded | degraded | degraded | G; physical key/app or hardware action pending. |
| SUPER + CTRL + E | emoji | degraded | degraded | degraded | G; physical key/app or hardware action pending. |
| Print | screenshot | degraded | degraded | degraded | G + N1 native/PNG capture family; picker interaction pending; save key captures focused output. |
| SUPER + SHIFT + S | screenshot | degraded | degraded | degraded | G + N1 native/PNG capture family; picker interaction pending; save key captures focused output. |
| SUPER + Print | save-screenshot | degraded | degraded | degraded | G + N1 native/PNG capture family; picker interaction pending; save key captures focused output. |
| SUPER + Q | put-away | degraded | missing | missing | Hyprland hide/trash/force-kill operation is not available. Capability/report; not emitted. |
| SUPER + SHIFT + Q | restore | degraded | missing | missing | Hyprland hide/trash/force-kill operation is not available. Capability/report; not emitted. |
| SUPER + ALT + Q | restore-all | degraded | missing | missing | Hyprland hide/trash/force-kill operation is not available. Capability/report; not emitted. |
| SUPER + CTRL + Q | close | degraded | degraded | degraded | G; physical key/app or hardware action pending. |
| SUPER + CTRL + SHIFT + Q | force-close | degraded | missing | missing | Hyprland hide/trash/force-kill operation is not available. Capability/report; not emitted. |
| SUPER + H | hide | degraded | missing | missing | Hyprland hide/trash/force-kill operation is not available. Capability/report; not emitted. |
| SUPER + SHIFT + H | unhide | degraded | missing | missing | Hyprland hide/trash/force-kill operation is not available. Capability/report; not emitted. |
| SUPER + ALT + H | application.launch | degraded | missing | missing | Hyprland hide/trash/force-kill operation is not available. Capability/report; not emitted. |
| SUPER + F | fullscreen | degraded | degraded | degraded | G + N0/N1 facade family; native layout/output-relative semantics. |
| SUPER + M | maximize | degraded | degraded | degraded | G + N0/N1 facade family; native layout/output-relative semantics. |
| SUPER + T | float | degraded | degraded | degraded | G + N0/N1 facade family; native layout/output-relative semantics. |
| SUPER + P | pin | degraded | missing | missing | No equivalent niri capability for window.pin. Capability/report; not emitted. |
| SUPER + SHIFT + P | window.pseudo | degraded | missing | missing | No equivalent niri capability for window.pseudo. Capability/report; not emitted. |
| SUPER + G | group | degraded | missing | degraded | G + N1 native tabs/focus; column grouping differs from Hyprland groups. |
| SUPER + CTRL + Page_Up | group-cycle | degraded | missing | degraded | G + N1 native tabs/focus; column grouping differs from Hyprland groups. |
| SUPER + CTRL + Page_Down | group-cycle | degraded | missing | degraded | G + N1 native tabs/focus; column grouping differs from Hyprland groups. |
| SUPER + J | split | degraded | missing | missing | No equivalent niri capability for layout.toggle-split. Capability/report; not emitted. |
| SUPER + mouse:273 | window.resize | degraded | missing | missing | Keep niri's native move/resize gestures; no pointer-hook substitution. Capability/report; not emitted. |
| ALT + TAB | cycle-window | degraded | missing | degraded | G + N1/U creation-order cycle within active workspace. |
| ALT + SHIFT + TAB | cycle-window | degraded | missing | degraded | G + N1/U creation-order cycle within active workspace. |
| SUPER + 1 | workspace | degraded | degraded | degraded | G + N0/N1 facade family; native layout/output-relative semantics. |
| SUPER + SHIFT + 1 | move-workspace | degraded | degraded | degraded | G + N0/N1 facade family; native layout/output-relative semantics. |
| SUPER + 2 | workspace | degraded | degraded | degraded | G + N0/N1 facade family; native layout/output-relative semantics. |
| SUPER + SHIFT + 2 | move-workspace | degraded | degraded | degraded | G + N0/N1 facade family; native layout/output-relative semantics. |
| SUPER + 3 | workspace | degraded | degraded | degraded | G + N0/N1 facade family; native layout/output-relative semantics. |
| SUPER + SHIFT + 3 | move-workspace | degraded | degraded | degraded | G + N0/N1 facade family; native layout/output-relative semantics. |
| SUPER + 4 | workspace | degraded | degraded | degraded | G + N0/N1 facade family; native layout/output-relative semantics. |
| SUPER + SHIFT + 4 | move-workspace | degraded | degraded | degraded | G + N0/N1 facade family; native layout/output-relative semantics. |
| SUPER + 5 | workspace | degraded | degraded | degraded | G + N0/N1 facade family; native layout/output-relative semantics. |
| SUPER + SHIFT + 5 | move-workspace | degraded | degraded | degraded | G + N0/N1 facade family; native layout/output-relative semantics. |
| SUPER + 6 | workspace | degraded | degraded | degraded | G + N0/N1 facade family; native layout/output-relative semantics. |
| SUPER + SHIFT + 6 | move-workspace | degraded | degraded | degraded | G + N0/N1 facade family; native layout/output-relative semantics. |
| SUPER + 7 | workspace | degraded | degraded | degraded | G + N0/N1 facade family; native layout/output-relative semantics. |
| SUPER + SHIFT + 7 | move-workspace | degraded | degraded | degraded | G + N0/N1 facade family; native layout/output-relative semantics. |
| SUPER + 8 | workspace | degraded | degraded | degraded | G + N0/N1 facade family; native layout/output-relative semantics. |
| SUPER + SHIFT + 8 | move-workspace | degraded | degraded | degraded | G + N0/N1 facade family; native layout/output-relative semantics. |
| SUPER + 9 | workspace | degraded | degraded | degraded | G + N0/N1 facade family; native layout/output-relative semantics. |
| SUPER + SHIFT + 9 | move-workspace | degraded | degraded | degraded | G + N0/N1 facade family; native layout/output-relative semantics. |
| SUPER + 0 | workspace | degraded | degraded | degraded | G + N0/N1 facade family; native layout/output-relative semantics. |
| SUPER + SHIFT + 0 | move-workspace | degraded | degraded | degraded | G + N0/N1 facade family; native layout/output-relative semantics. |
| SUPER + CTRL + right | next-workspace | degraded | degraded | degraded | G + N0/N1 facade family; native layout/output-relative semantics. |
| SUPER + CTRL + left | previous-workspace | degraded | degraded | degraded | G + N0/N1 facade family; native layout/output-relative semantics. |
| SUPER + mouse_down | workspace.switch | degraded | degraded | degraded | G + N0/N1 facade family; native layout/output-relative semantics. |
| SUPER + mouse_up | workspace.switch | degraded | degraded | degraded | G + N0/N1 facade family; native layout/output-relative semantics. |
| SUPER + CTRL + SHIFT + left | window.move | degraded | degraded | degraded | G + N0/N1 facade family; native layout/output-relative semantics. |
| SUPER + CTRL + SHIFT + right | window.move | degraded | degraded | degraded | G + N0/N1 facade family; native layout/output-relative semantics. |
| SUPER + S | scratchpad | degraded | missing | missing | No equivalent niri capability for workspace.scratchpad. Capability/report; not emitted. |
| SUPER + CTRL + S | move-scratchpad | degraded | missing | missing | Niri has no Hyprland special/scratchpad workspaces. Capability/report; not emitted. |
| SUPER + left | focus | degraded | degraded | degraded | G + N0/N1 facade family; native layout/output-relative semantics. |
| SUPER + SHIFT + left | window.move | degraded | degraded | degraded | G + N0/N1 facade family; native layout/output-relative semantics. |
| SUPER + ALT + left | window.resize | degraded | degraded | degraded | G + N0/N1 facade family; native layout/output-relative semantics. |
| SUPER + right | focus | degraded | degraded | degraded | G + N0/N1 facade family; native layout/output-relative semantics. |
| SUPER + SHIFT + right | window.move | degraded | degraded | degraded | G + N0/N1 facade family; native layout/output-relative semantics. |
| SUPER + ALT + right | window.resize | degraded | degraded | degraded | G + N0/N1 facade family; native layout/output-relative semantics. |
| SUPER + up | focus | degraded | degraded | degraded | G + N0/N1 facade family; native layout/output-relative semantics. |
| SUPER + SHIFT + up | window.move | degraded | degraded | degraded | G + N0/N1 facade family; native layout/output-relative semantics. |
| SUPER + ALT + up | window.resize | degraded | degraded | degraded | G + N0/N1 facade family; native layout/output-relative semantics. |
| SUPER + down | focus | degraded | degraded | degraded | G + N0/N1 facade family; native layout/output-relative semantics. |
| SUPER + SHIFT + down | window.move | degraded | degraded | degraded | G + N0/N1 facade family; native layout/output-relative semantics. |
| SUPER + ALT + down | window.resize | degraded | degraded | degraded | G + N0/N1 facade family; native layout/output-relative semantics. |
| SUPER + R | keymap.switch | degraded | missing | missing | No equivalent niri capability for keymap.switch. Capability/report; not emitted. |
| left / resize | window.resize | degraded | missing | missing | No equivalent niri capability for window.resize. Capability/report; not emitted. |
| right / resize | window.resize | degraded | missing | missing | No equivalent niri capability for window.resize. Capability/report; not emitted. |
| up / resize | window.resize | degraded | missing | missing | No equivalent niri capability for window.resize. Capability/report; not emitted. |
| down / resize | window.resize | degraded | missing | missing | No equivalent niri capability for window.resize. Capability/report; not emitted. |
| escape / resize | keymap.switch | degraded | missing | missing | No equivalent niri capability for keymap.switch. Capability/report; not emitted. |
| return / resize | keymap.switch | degraded | missing | missing | No equivalent niri capability for keymap.switch. Capability/report; not emitted. |
| SUPER + R / resize | keymap.switch | degraded | missing | missing | No equivalent niri capability for keymap.switch. Capability/report; not emitted. |
| SUPER + SPACE | launcher | degraded | degraded | degraded | G + N0/N1 corresponding shell IPC; actual chord remains unverified. |
| SUPER + TAB | overview | degraded | degraded | degraded | G + N0/N1 corresponding shell IPC; actual chord remains unverified. |
| SUPER + V | clipboard | degraded | degraded | degraded | G + N0/N1 corresponding shell IPC; actual chord remains unverified. |
| SUPER + W | wallpaper | degraded | degraded | degraded | G + N0/N1 corresponding shell IPC; actual chord remains unverified. |
| SUPER + Z | application.launch | degraded | degraded | degraded | G; physical key/app or hardware action pending. |
| SUPER + O | application.launch | degraded | degraded | degraded | G; physical key/app or hardware action pending. |
| SUPER + I | settings | degraded | degraded | degraded | G; physical key/app or hardware action pending. |
| SUPER + CTRL + C | application.launch | degraded | degraded | degraded | G; physical key/app or hardware action pending. |
| SUPER + X | power | degraded | degraded | degraded | G; physical key/app or hardware action pending. |
| SUPER + ESCAPE | power | degraded | degraded | degraded | G; physical key/app or hardware action pending. |
| SUPER + L | lock | degraded | degraded | degraded | G; physical key/app or hardware action pending. |
| SUPER + SHIFT + R | compositor.reload | degraded | degraded | degraded | G + N0/N1 facade family; native layout/output-relative semantics. |
| SUPER + RETURN | terminal | degraded | degraded | degraded | G; physical key/app or hardware action pending. |
| SUPER + SHIFT + B | browser | degraded | degraded | degraded | G; physical key/app or hardware action pending. |
| SUPER + E | files | degraded | degraded | degraded | G; physical key/app or hardware action pending. |
| SUPER + B | application.launch | degraded | degraded | degraded | G; physical key/app or hardware action pending. |
| SUPER + N | application.launch | degraded | degraded | degraded | G; physical key/app or hardware action pending. |
| SUPER + CTRL + SPACE | palette | degraded | degraded | degraded | G + N0/N1 corresponding shell IPC; actual chord remains unverified. |
| SUPER + K | shortcuts | degraded | degraded | degraded | G; physical key/app or hardware action pending. |
| SUPER + F1 | shortcuts | degraded | degraded | degraded | G; physical key/app or hardware action pending. |
| SUPER + A | ai-sidebar | degraded | degraded | degraded | G; physical key/app or hardware action pending. |
| Escape | shell.shortcut | degraded | missing | missing | Niri bindings consume input; non-consuming hooks are unsupported. Capability/report; not emitted. |
| SUPER + CTRL + D | detach-panel | degraded | degraded | degraded | G + N0/N1 corresponding shell IPC; actual chord remains unverified. |
| SUPER + mouse:272 | move-panel-or-window | degraded | missing | missing | No equivalent niri action for panel.drag. Capability/report; not emitted. |
| mouse:272 | finish-panel-or-window-drag | degraded | missing | missing | No equivalent niri capability for panel.drag.finish. Capability/report; not emitted. |

## Other compositor declarations

Every remaining contract declaration is listed here. G proves compilation and
parse acceptance; window opening, layer rendering, pointer gesture and event
semantics require their own acceptance and are not inferred from a valid file.

| Source declaration | Hyprland | Niri before | Niri after | Evidence / limit |
|---|---|---|---|---|
| windowrule.lua: window-rule nacre-thunar-files | degraded | degraded | degraded | G fixed/proportional sizing now emitted. Niri proportions use working area; centering is native default. |
| windowrule.lua: window-rule nacre-dolphin-files | degraded | degraded | degraded | G fixed/proportional sizing now emitted. Niri proportions use working area; centering is native default. |
| nacre.lua: layer-rule nacre-overlay-glass | degraded | missing | degraded | G bar-layer blur; full-output drawers excluded because shaped blur is unavailable. |
| nacre.lua: window-rule nacre-lock-preview | degraded | degraded | degraded | G fixed/proportional sizing now emitted. Niri proportions use working area; centering is native default. |
| nacre.lua: window-rule nacre-audio-mixer | degraded | degraded | degraded | G fixed/proportional sizing now emitted. Niri proportions use working area; centering is native default. |
| nacre.lua: window-rule nacre-bluetooth-manager | degraded | degraded | degraded | G fixed/proportional sizing now emitted. Niri proportions use working area; centering is native default. |
| nacre.lua: window-rule nacre-network-editor | degraded | degraded | degraded | G fixed/proportional sizing now emitted. Niri proportions use working area; centering is native default. |
| nacre.lua: window-rule nacre-share-picker | degraded | degraded | degraded | G fixed/proportional sizing now emitted. Niri proportions use working area; centering is native default. |
| nacre.lua: window-rule nacre-picture-in-picture | degraded | degraded | degraded | G match/float/corner/routing rules; niri pinned effect remains unsupported. |
| nacre.lua: window-rule nacre-native-controls | degraded | degraded | degraded | G fixed/proportional sizing now emitted. Niri proportions use working area; centering is native default. |
| nacre.lua: backend-config 9 | degraded | missing | missing | Backend-specific declaration; G/report. No native equivalent is fabricated. |
| nacre.lua: window-rule nacre-settings-app | degraded | degraded | degraded | G match/float/corner/routing rules; niri pinned effect remains unsupported. |
| nacre.lua: window-rule nacre-colors-app | degraded | degraded | degraded | G match/float/corner/routing rules; niri pinned effect remains unsupported. |
| nacre.lua: window-rule nacre-welcome | degraded | degraded | degraded | G match/float/corner/routing rules; niri pinned effect remains unsupported. |
| brain.lua: window-rule nacre-brain | degraded | degraded | degraded | G match/float/corner/routing rules; niri pinned effect remains unsupported. |
| brain.lua: window-rule nacre-brain-chrome | degraded | degraded | degraded | G match/float/corner/routing rules; niri pinned effect remains unsupported. |
| brain.lua: window-rule nacre-obsidian | degraded | degraded | degraded | G match/float/corner/routing rules; niri pinned effect remains unsupported. |
| brain.lua: window-rule brain-auth-handoff | degraded | degraded | degraded | G fixed/proportional sizing now emitted. Niri proportions use working area; centering is native default. |
| shortcuts.lua: window-rule 7 | degraded | degraded | degraded | G match/float/corner/routing rules; niri pinned effect remains unsupported. |
| shortcuts.lua: unbind SUPER + mouse:272 | degraded | missing | missing | Backend-specific declaration; G/report. No native equivalent is fabricated. |
| shortcuts.lua: extension panel.drag | degraded | missing | missing | Backend-specific declaration; G/report. No native equivalent is fabricated. |
| cursor.lua: extension cursor.session | degraded | missing | missing | Backend-specific declaration; G/report. No native equivalent is fabricated. |
| cursor.lua: event session.ready | degraded | missing | missing | Backend-specific declaration; G/report. No native equivalent is fabricated. |

## Every shell IPC entry point

Dynamic targets are shown as their source expressions. `works` is restricted to
methods asserted by N0/N1; `degraded` methods are present but not independently
exercised in the nested probe (U may cover them). Service/hardware writes, account
activity and real-session actions are deliberately not invoked.

| Target | Method | Hyprland | Niri before | Niri after | Evidence |
|---|---|---|---|---|---|
| leftDrawer | section | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| leftDrawer | state | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| nacre | state | works | works | works | H/N0/N1 state/route assertion. |
| nacre | restoreViews | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| nacre | preview | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| nacre | previewDashboard | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| nacre | controls | works | works | works | H/N0/N1 state/route assertion. |
| nacre | previewFeedback | works | works | works | H/N0/N1 state/route assertion. |
| nacre | previewSliders | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| nacre | previewLeft | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| nacre | mode | works | works | works | H/N0/N1 state/route assertion. |
| nacre | menu | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| nacre | settings | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| nacre | left | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| nacre | hide | works | works | works | H/N0/N1 state/route assertion. |
| nacre | launcher | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| nacre | session | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| nacre | popout | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| nacre | close | works | works | works | H/N0/N1 state/route assertion. |
| nacre | popupState | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| nacre | galleryStep | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| nacre | galleryState | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| nacre | workspace | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| nacre | tab | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| drawers | toggle | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| drawers | list | works | works | works | H/N0/N1 state/route assertion. |
| "wallpaperMotion-" + root.screenName | state | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| "wallpaperFrame-" + root.screenName | state | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| "windowTitle-" + (root.monitor?.name \|\| "default") | state | degraded | degraded | degraded | V actual QML caption with companion; stock hover missing and explained. |
| "barStatus-" + root.screenName | state | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| leftEdge | state | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| frame | state | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| root.detached ? "leftDrawerWindow" + root.visibilities.windowKey : "leftDrawer-" + root.screen.name | section | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| root.detached ? "leftDrawerWindow" + root.visibilities.windowKey : "leftDrawer-" + root.screen.name | state | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| overview | state | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| overview | move | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| root.detached ? "featureSearchWindow" + root.visibilities.windowKey : "featureSearch" | query | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| root.detached ? "featureSearchWindow" + root.visibilities.windowKey : "featureSearch" | state | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| root.detached ? "appBrowserWindow" + root.visibilities.windowKey : "appBrowser" | selectCategory | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| root.detached ? "appBrowserWindow" + root.visibilities.windowKey : "appBrowser" | openRoute | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| root.detached ? "appBrowserWindow" + root.visibilities.windowKey : "appBrowser" | state | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| "header-" + root.screen.name | state | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| chatTitle | state | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| desktopActions | run | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| extras | counts | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| extras | refresh | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| extras | search | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| desktopSettings | state | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| desktopSettings | set | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| displayRecovery | refresh | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| displayRecovery | state | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| lockWidgets | state | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| batteryAlerts | state | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| brightness | step | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| brightness | set | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| brightness | state | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| colorsApp | open | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| colorsApp | preview | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| colorsApp | close | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| colorsApp | state | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| paletteState | state | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| dashboardState | state | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| keyboardLight | step | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| keyboardLight | set | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| keyboardLight | state | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| menuPrompt | open | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| menuPrompt | cancel | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| menuPrompt | state | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| networkStatus | state | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| networkStatus | refresh | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| notifs | counts | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| notifs | clear | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| notifs | dismissKey | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| notifs | clearHistory | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| pearls | open | degraded | works | works | H/N0/N1 state/route assertion. |
| pearls | detach | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| pearls | dock | degraded | works | works | H/N0/N1 state/route assertion. |
| pearls | close | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| pearls | gesture | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| pearls | state | works | works | works | H/N0/N1 state/route assertion. |
| mpris | getActive | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| mpris | list | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| mpris | play | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| mpris | pause | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| mpris | playPause | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| mpris | previous | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| mpris | next | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| mpris | stop | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| presentationState | state | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| settingsApp | open | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| settingsApp | close | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| settingsApp | state | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| settingsView | open | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| settingsView | state | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| resources | state | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| clock | state | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| wallpaper | get | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| wallpaper | set | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| wallpaper | configure | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| wallpaper | next | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| wallpaper | previous | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| wallpaper | toggleMotion | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| wallpaper | list | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| wallpaper | state | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| weatherStatus | state | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| welcomeApp | open | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| welcomeApp | close | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| welcomeApp | refresh | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| welcomeApp | setStartup | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| welcomeApp | route | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| welcomeApp | selectDemo | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| welcomeApp | restoreDemo | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| welcomeApp | themeDemo | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| welcomeApp | state | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| sidebarChat | state | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| sidebarChat | send | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| sidebarChat | stop | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| wallpaperPlayback | state | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| compositorState | state | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |
| nacreCompositor | shortcut | degraded | degraded | degraded | Published route; U fixtures where applicable; nested side effect not independently accepted. |

## Launcher feature inventory

All catalog entries (including sections) are enumerated. The shared launcher,
search and navigation surface passes N0/N1. Opening a route does not verify its
external action; writes and provider-specific behavior remain degraded pending
service/hardware/account acceptance. Optional software is not installed by a test.

| Catalog feature | Operation | Hyprland | Niri before | Niri after | Evidence |
|---|---|---|---|---|---|
| home | group | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| apps | group | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| actions | group | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| style | group | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| setup | group | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| software | group | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| health | group | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| learn | group | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| power | group | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| actions.capture | group | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| actions.capture.area | tool | degraded | degraded | degraded | N0/N1 PNG family; native window capture now avoids geometry assumptions. Area interaction/multi-output pending. |
| actions.capture.screen | tool | degraded | degraded | degraded | N0/N1 PNG family; native window capture now avoids geometry assumptions. Area interaction/multi-output pending. |
| actions.capture.window | tool | degraded | degraded | degraded | N0/N1 PNG family; native window capture now avoids geometry assumptions. Area interaction/multi-output pending. |
| actions.capture.record | tool | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| actions.capture.stop | tool | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| actions.capture.text | tool | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| actions.capture.qr | tool | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| actions.capture.color | tool | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| actions.clipboard | group | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| actions.emoji | group | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| actions.calculator | calculator | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| actions.windows | group | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| actions.reminders | group | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| actions.reminders.new | form | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| actions.reminders.focus | tool | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| actions.assistant | group | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| actions.assistant.ai | legacy | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| actions.assistant.new | legacy | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| actions.assistant.resume | legacy | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| actions.assistant.chats | legacy | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| actions.assistant.brain | legacy | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| actions.assistant.capture | legacy | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| actions.assistant.search | scope | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| actions.assistant.ask | scope | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| actions.controls | group | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| actions.controls.dnd | toggle | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| actions.controls.night-light | toggle | degraded | degraded | degraded | U owned controller; nested gamma absent, UI disabled with reason; real colors pending. |
| actions.controls.stay-awake | toggle | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| actions.controls.mute | toggle | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| actions.controls.mic-mute | toggle | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| actions.controls.wifi | toggle | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| actions.controls.bluetooth | toggle | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| style.wallpapers | group | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| style.wallpapers.picker | route | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| style.wallpapers.next | next-wallpaper | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| style.personality | group | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| style.personality.natural | personality | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| style.personality.pigment | personality | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| style.personality.harmony | personality | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| style.personality.pop | personality | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| style.personality.mist | personality | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| style.personality.vivid | personality | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| style.personality.pearl | personality | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| style.personality.tide | personality | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| style.background | group | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| style.background.accent | background | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| style.background.wallpaper | background | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| style.appearance | group | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| style.appearance.dark | palette-mode | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| style.appearance.light | palette-mode | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| style.appearance.auto | palette-mode | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| style.accents | group | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| style.fonts | group | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| style.animations | setting-toggle | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| style.blur | setting-toggle | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| style.shadow | setting-toggle | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| style.colors | legacy | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| setup.appearance | settings | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| setup.desktop | settings | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| setup.displays | settings | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| setup.sound | settings | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| setup.network | settings | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| setup.bluetooth | settings | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| setup.notifications | settings | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| setup.workflows | settings | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| setup.lock | settings | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| setup.time | settings | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| setup.ai | settings | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| setup.wifi-networks | group | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| setup.bluetooth-devices | group | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| setup.shortcuts | config | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| setup.defaults | config | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| software.discover | group | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| software.packages | group | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| software.installed | group | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| software.webapps | group | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| software.webapps.new | form | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| health.updates | tool | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| health.nacre | settings | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| health.check | maintenance | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| health.status | info | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| health.support-report | tool | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| health.logs | tool | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| health.restart | maintenance | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| health.rollback | settings | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| learn.shortcuts | group | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| learn.welcome | legacy | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| learn.nacre | info | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| learn.hyprland | url | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| learn.arch | url | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| learn.quickshell | url | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| learn.news | url | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| power.lock | power | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| power.suspend | power | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| power.logout | power | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| power.reboot | power | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| power.poweroff | power | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| actions.files | legacy | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| actions.terminal | legacy | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| actions.apps | route | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| actions.controls.profile | group | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| actions.controls.profile.power-saver | power-profile | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| actions.controls.profile.balanced | power-profile | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| actions.controls.profile.performance | power-profile | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| actions.media | group | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| actions.media.play-pause | media | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| actions.media.next | media | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| actions.media.previous | media | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |
| actions.find-files | scope | degraded | degraded | degraded | N0/N1 launcher opens; U catalog/backend route fixtures. External side effect not invoked. |

The software catalog contains optional installation choices, not compositor
features; they use the same shared package route. No package operation was run.

## Owner-run acceptance — explicit approval required

These steps are instructions, not completed tests. Do them only in the intended
real niri session, after retaining a working TTY/login recovery path.

1. **Keyboard and panels:** open three ordinary windows. Test Super+Space, Super+Tab,
   Super+1/2, Super+Shift+1/2, Super+Ctrl+Left/Right, Alt+Tab, Super+G and tab cycling.
   Confirm numbers follow the focused output, IDs survive reorder, and tabs stay
   in their column. Super+Ctrl+Q closes immediately; undo/trash, minimize,
   scratchpad and sticky keys are intentionally absent. Drag a pearl, focus its
   input, resize it, then Dock or use Super+Ctrl+D; confirm state and keyboard
   focus return. With stock niri, the overview caption explains missing hover;
   with the companion, hover each public test window and verify its title.
2. **Capture and sharing:** Print selects an area in niri's picker; Escape cancels.
   Super+Print saves the focused output. Test launcher area, full and window
   capture; paste each PNG into a temporary document. With two outputs, confirm
   the full helper captures both and the native save key captures only the focused
   output. In a browser/OBS, share a monitor and a window, verify the intended
   source and cursor, stop sharing, and repeat. Confirm the GNOME portal backend
   is installed and selected by niri's portal preferences; do not restart the live
   portal as part of an unattended test.
3. **Lock then idle:** use Super+L, verify all outputs are covered, and unlock with
   the correct password. Only after this passes, enable the existing idle policy
   and wait through its configured timeout. Check Stay awake prevents automatic
   idle actions, then turn it off and repeat. Never disable inhibitors or bypass
   authentication to get a passing result.
4. **Suspend/lid:** save work, lock manually, then suspend from the power menu.
   Resume and confirm the lock is still present before entering the password;
   check display, keyboard, pointer, audio, network and pearl state. Repeat by
   closing/opening the lid with the built-in display only, then with an external
   display attached. Record whether lid policy suspends or disables the panel;
   do not assume docked behavior matches battery-only behavior.
5. **Outputs/hotplug:** attach an external display. Extend right, then left; change
   scale to 1.25 or 1.5. First let the 20-second guard revert, then repeat and Keep.
   Verify pointer crossings, workspace ownership, pearl placement and lock
   coverage. Unplug during a pending change, reconnect, and verify restoration;
   repeat with lid/suspend. Mirroring is disabled because niri has no native
   output-mirroring primitive. Niri's layout and rollback state must not overwrite
   Hyprland's saved display layout.
6. **Night light and startup:** enable Night light and visually verify each output
   warms; disable it and verify neutral colors. Test logout reset and check an
   externally started controller is left alone. Test an X11 application through
   xwayland-satellite, including input/focus/close; verify it exits on logout.
   Log in twice with configured startup apps and verify exactly one instance on
   each named workspace. Check shell, idle and portal service lifetime after logout.

Full daily-driver acceptance remains incomplete until these hardware/service
checks pass. The packaged setup/dependency gate is retained; this source audit
does not upgrade the machine or authorize live-session testing.
