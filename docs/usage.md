# Using Nacre

Open the launcher with Super+Space and type what you want. Normal search includes
apps, desktop actions, open windows, clipboard history, emoji, saved launcher
items, files and optional Brain notes. Files and notes start searching after two
characters; apps and commands appear immediately. The Everything filter is the
default each time you open the launcher. Its other choices narrow results when
useful, and also work from deep menu sections.

Ordinary launcher search also offers installation for missing common applications,
including Discord, Google Chrome, Steam and OBS Studio. Installed applications
keep their launch result; their install offer is omitted. The catalog covers
browsers, messaging, office, creative, gaming, developer and utility applications.
Selecting an install result opens the existing package-manager terminal. AUR
applications require paru or yay; the launcher explains that requirement when
neither helper is available. Searching installs nothing automatically.

Enter opens the selected result. Tab opens item actions: switch to an app's
existing window, pin or delete a clipboard item, or open a file's folder, copy
its path or file, share it, annotate an image or convert media. Image and small
text files have local previews. Web searches and AI questions run only when you
select them. `>` scopes actions, `=` opens the calculator, `#` scopes optional
Brain notes, `:` scopes files and `?` opens optional AI. These shortcuts are
optional; normal typing already searches local content. Manage favorites on
hover or with Ctrl+D. Super+I opens Settings; Super+K opens the shortcut list.

Actions → Saved launcher items provides guided forms for actions, text snippets,
bookmarks, named web searches, workspace recipes and extra search folders.
Arguments are entered one per line and passed literally. Search finds saved
items automatically. Registered local projects also appear as workspace choices.
A recipe opens its editor, terminal and optional documentation; running recipe
units prevent duplicate editor/terminal launches. Default project choices use
Code and Kitty; save a recipe to use different programs.

File search includes Documents, Downloads, Pictures, Desktop, Projects and local
folders in the project registry. Extra folders are private preferences. Search
uses `fd`, respects its normal hidden-file and ignore rules, and limits results
and run time. File previews stay inside configured folders and avoid hidden
credential files. Emoji search uses Unicode 17 names and CLDR keywords; recent and favorite
emoji appear first in the emoji section.

Actions → Capture offers Record an area and Record screen. Record screen captures
the entire focused monitor. Choose No sound, System audio, or System audio and
microphone, then Start recording. Sound modes need PulseAudio compatibility
(`pactl`); devices retain their current mute settings. The launcher and bar show
elapsed time and Stop. Both modes save to `Videos/Recordings`. Finished captures
have notification actions and appear in Capture for opening, revealing, copying,
annotation and conversion. Stop remains available during a conversion. Media
conversion writes a new file alongside the original.

Sharing uses optional LocalSend; annotation uses Satty; compression and image/GIF
conversion use FFmpeg. The optional Voice typing actions use your configured
Voxtype daemon. Configure its model and service before recording dictation.
Optional tools are available through Recommended software; adding launcher
features does not install them or download voice models automatically.

Type `=` or choose Calculator for a keypad with arithmetic, parentheses, sign
change, square and square root. Type expressions directly or use the buttons;
the result updates as you work. The `=` button keeps the answer ready for the
next operation. Copy saves the result to the clipboard while keeping the
calculator open; Enter copies it and closes the attached launcher.

Colors previews wallpaper palettes before applying. Natural keeps the image's
balance; Pop brings out contrasting details. Other styles offer softer or more
vivid palettes. Dark/light and background tint are separate choices. Applying
publishes the wallpaper and colors. Static and animated wallpapers share the
picker; motion and battery preferences live in Settings. Welcome introduces the
main controls on first use.

Super+Z hides or shows the shell frame and panels. The wallpaper stays visible,
and tiled windows gain the freed space. Showing the frame restores its reserved
space without changing configured gaps. Super+Q closes the current window;
put-away/undo is a separate Hyprland profile opt-in. Super+L locks, and
Super+Escape or Super+X opens the power menu.

The control center groups volume, supported brightness controls and everyday
toggles on one compact page. Media stays outside the control center. Ethernet
connections show a cable icon and wired status; click to view connection details.
Wi-Fi, Bluetooth and power tiles open their detail pages with one full-card
button; their switches and profile choices stay inside those pages. Click other
toggle tiles to switch them; action tiles open their tool. Right-click or long-press
a toggle tile for details (keyboard: Menu or Shift+F10). Back or Escape returns to the overview. Light, Dark and Auto share
one appearance selector. The attached panel shares the right wall's surface.
The overview includes the two newest notifications below the controls; expand
individual cards or open the full history with its arrow. The top battery row
includes edit, settings and power. Lock, log out, restart
and power off stay on its Power page; the bar power button and session shortcuts
open that page too. Log out, restart and power off require inline confirmation.
Wi-Fi passwords, Bluetooth devices, audio inputs/outputs and the
application mixer stay inside the panel. Power details show supported display
and keyboard brightness controls only.
Unsupported night light is hidden. Edit can add, remove, and reorder every tile,
slider, and the appearance selector, with a Reset action. Optional controls include
microphone and keyboard sliders, airplane mode, VPN, recording, mic mute,
screenshot, color picker, clipboard, wallpaper, updates, and display settings.
Edit lists detected microphone and keyboard sliders first; adding one places it
beside its related output or display slider. Layouts are saved separately from
the main desktop settings and keep their order across sessions; existing optional
tile preferences migrate automatically. The same controls work in a detached pearl.
Hover a bar status icon or the clock for a dropdown after 350 ms. Status
summaries are read-only; the calendar keeps its month controls. Clicking the
clock pins its calendar for keyboard use until Escape or dismissal. Status hover
highlights fill their segment of the shared pill, following its rounded ends.
A palette-colored
highlight and filling underline show the wait. The left and right frame edges have small arrows inside the existing border with the same delayed
fill; clicks open or close their panels immediately. The middle title and top lip wait
350 ms before opening the dashboard; leaving cancels it, while a click opens
immediately. The fill follows display frames and stays complete briefly before the panel
unfolds; delayed frames cannot skip the remaining fill. Reduced motion keeps the delay with a static highlight. Dropdowns
unfold from the bar using the same frame and motion as other Nacre popups. Clicking
a status icon opens its control-center detail page; the bell opens notification history inside the same
panel. Notification history can also be added as a tile in Edit, or opened by
right-clicking Do Not Disturb. Pointer-opened controls
close after leaving the panel; keyboard-opened controls keep their focus. Wi-Fi
shows cached networks immediately and refreshes passively; use Scan for a fresh
scan without waiting for one on every open. Scroll sound for volume
or battery for brightness. Right-click sound, Wi-Fi or Bluetooth for a quick
toggle. Sound details include microphone volume; brightness details include
keyboard backlight. Power mode opens the three profile choices without changing
the active profile, and session actions use a compact inline page.

Notifications can be reviewed in history; Do not disturb suppresses popups.
Night light warms display colors, and Stay awake temporarily inhibits idle
actions. On niri, night light needs wlsunset and output gamma control. Niri display
changes support scale, mode and extension with rollback; mirroring is unavailable.
Read [Nacre on niri](../README.md#nacre-on-niri) for limits.

New Nacre sessions use a window border width of 3. On Niri this is measured in
logical pixels: each display's scale determines its physical thickness, so scale
1 draws 3 pixels and scale 2 draws 6. Niri handles fractional scales and moving
windows between displays. Adjust **Settings → Desktop → Window border width**
to suit your preference; saved widths remain intact when you update Nacre.
See [Niri's border configuration](https://niri-wm.github.io/niri/Configuration%3A-Layout.html#width).

[Optional AI and Brain](optional-apps.md) need separate setup. Supported panels
can [open as ordinary windows](detachable-panels.md): Super+Ctrl+D detaches the
open panel. Hyprland also supports holding Super and dragging it away from the edge.

## Shortcut reference

**Super** is usually the Windows key. These tables are generated defaults; private
overrides may differ. Hyprland's launcher shortcut list reads current session
bindings; niri shows generated bindings because private binding introspection is
unavailable. Hardware keys need the corresponding device and installed tools.
For custom bindings and the trash opt-in, see [configuration](configuration.md).

<!-- BEGIN GENERATED KEYBINDINGS -->

<!-- Generated by tools/generate_usage_keybindings.py; edit the contract, not these tables. -->

## Hyprland shortcuts

| Keys | Action |
| --- | --- |
| Audio mute key | Mute audio |
| Volume down key | Lower volume |
| Volume up key | Raise volume |
| Microphone mute key | Mute microphone |
| Power key / Super + L | Lock the session |
| Brightness down key | Lower display brightness |
| Brightness up key | Raise display brightness |
| Keyboard backlight up key | Raise keyboard backlight |
| Keyboard backlight down key | Lower keyboard backlight |
| Super + . / Super + Ctrl + E | Emoji picker |
| Print Screen / Super + Shift + S | Capture an area to the clipboard |
| Super + Print Screen | Save a desktop screenshot |
| Super + Q | Close the window |
| Super + H | Hide the window |
| Super + Shift + H | Restore the last hidden window |
| Super + Alt + H | Restore hidden windows on this workspace |
| Super + F | Toggle fullscreen |
| Super + M | Toggle maximization |
| Super + T | Toggle floating |
| Super + P | Keep the window on all workspaces |
| Super + Shift + P | Toggle pseudo tiling |
| Super + G | Toggle window group |
| Super + Ctrl + Page Up | Previous window in window group |
| Super + Ctrl + Page Down | Next window in window group |
| Super + J | Toggle split direction |
| Super + right mouse button | Resize the window by dragging |
| Alt + Tab | Cycle windows forward |
| Alt + Shift + Tab | Cycle windows backward |
| Super + 1–9 / Super + 0 | Switch to workspace 1–10 (0 selects 10) |
| Super + Shift + 1–9 / Super + Shift + 0 | Move the window to workspace 1–10 (0 selects 10) |
| Super + Ctrl + Right / Super + scroll down | Switch to next workspace |
| Super + Ctrl + Left / Super + scroll up | Switch to previous workspace |
| Super + Ctrl + Shift + Left | Move the window to previous workspace |
| Super + Ctrl + Shift + Right | Move the window to next workspace |
| Super + S | Toggle scratchpad |
| Super + Ctrl + S | Move the window to scratchpad |
| Super + Left | Focus left |
| Super + Shift + Left | Move the window left |
| Super + Alt + Left / Left (resize mode) | Decrease width |
| Super + Right | Focus right |
| Super + Shift + Right | Move the window right |
| Super + Alt + Right / Right (resize mode) | Increase width |
| Super + Up | Focus up |
| Super + Shift + Up | Move the window up |
| Super + Alt + Up / Up (resize mode) | Decrease height |
| Super + Down | Focus down |
| Super + Shift + Down | Move the window down |
| Super + Alt + Down / Down (resize mode) | Increase height |
| Super + R | Enter resize mode |
| Escape (resize mode) / Enter (resize mode) / Super + R (resize mode) | Exit resize mode |
| Super + Space | Universal launcher |
| Super + Tab | Window overview |
| Super + V | Clipboard history |
| Super + W | Wallpaper picker |
| Super + Z | Hide or show the shell frame and panels |
| Super + O | Toggle the main panel |
| Super + I | Nacre Settings |
| Super + Ctrl + C | Capture menu |
| Super + X / Super + Escape | Power menu |
| Super + Shift + R | Reload compositor configuration |
| Super + Enter | Default terminal |
| Super + Shift + B | Default browser |
| Super + E | Default file manager |
| Super + B | Optional Brain |
| Super + N | Capture a note in optional Brain |
| Super + Ctrl + Space | Command palette |
| Super + K / Super + F1 | Shortcut list |
| Super + A | Optional AI sidebar |
| Escape | Dismiss hover panels (also passes Escape to the app) |
| Super + Ctrl + D | Detach the open panel |
| Super + left mouse button | Move a panel or window by dragging |
| left mouse button (release) | Finish dragging a panel or window |

## Niri shortcuts

Number keys select existing workspaces on the focused output; their numbers can change.

| Keys | Action |
| --- | --- |
| Audio mute key | Mute audio |
| Volume down key | Lower volume |
| Volume up key | Raise volume |
| Microphone mute key | Mute microphone |
| Power key / Super + L | Lock the session |
| Brightness down key | Lower display brightness |
| Brightness up key | Raise display brightness |
| Keyboard backlight up key | Raise keyboard backlight |
| Keyboard backlight down key | Lower keyboard backlight |
| Super + . / Super + Ctrl + E | Emoji picker |
| Print Screen / Super + Shift + S | Open niri's screenshot picker |
| Super + Print Screen | Save a screenshot of the focused output |
| Super + Q | Close the window |
| Super + F | Toggle fullscreen |
| Super + M | Toggle maximization |
| Super + T | Toggle floating |
| Super + G | Toggle column tabs |
| Super + Ctrl + Page Up | Previous window in column tabs |
| Super + Ctrl + Page Down | Next window in column tabs |
| Alt + Tab | Cycle windows forward |
| Alt + Shift + Tab | Cycle windows backward |
| Super + 1–9 / Super + 0 | Switch to workspace 1–10 (0 selects 10) |
| Super + Shift + 1–9 / Super + Shift + 0 | Move the window to workspace 1–10 (0 selects 10) |
| Super + Ctrl + Right / Super + scroll down | Switch to next workspace |
| Super + Ctrl + Left / Super + scroll up | Switch to previous workspace |
| Super + Ctrl + Shift + Left | Move the window to previous workspace |
| Super + Ctrl + Shift + Right | Move the window to next workspace |
| Super + Left | Focus left |
| Super + Shift + Left | Move the window left |
| Super + Alt + Left | Decrease width |
| Super + Right | Focus right |
| Super + Shift + Right | Move the window right |
| Super + Alt + Right | Increase width |
| Super + Up | Focus up |
| Super + Shift + Up | Move the window up |
| Super + Alt + Up | Decrease height |
| Super + Down | Focus down |
| Super + Shift + Down | Move the window down |
| Super + Alt + Down | Increase height |
| Super + Space | Universal launcher |
| Super + Tab | Window overview |
| Super + V | Clipboard history |
| Super + W | Wallpaper picker |
| Super + Z | Hide or show the shell frame and panels |
| Super + O | Toggle the main panel |
| Super + I | Nacre Settings |
| Super + Ctrl + C | Capture menu |
| Super + X / Super + Escape | Power menu |
| Super + Shift + R | Reload compositor configuration |
| Super + Enter | Default terminal |
| Super + Shift + B | Default browser |
| Super + E | Default file manager |
| Super + B | Optional Brain |
| Super + N | Capture a note in optional Brain |
| Super + Ctrl + Space | Command palette |
| Super + K / Super + F1 | Shortcut list |
| Super + A | Optional AI sidebar |
| Super + Ctrl + D | Detach the open panel |

## Personal trash shortcuts (opt-in, Hyprland only)

Enable `windowTrashBindings` in your profile. These are absent from the default profile
and unavailable on niri. Super + Q still closes immediately. After the put-away
deadline, a window that resists closing can be forcibly terminated; save work first.

| Keys | Action |
| --- | --- |
| Super + Ctrl + Q | Put away the window; close after 30 seconds (opt-in) |
| Super + Shift + Q | Undo the last put-away within 30 seconds (opt-in) |
| Super + Alt + Q | Restore all windows still put away (opt-in) |
| Super + Ctrl + Shift + Q | Force close the window (opt-in) |

<!-- END GENERATED KEYBINDINGS -->

### Optional dashboard pages

The default dashboard tabs are Dashboard, Media, Weather and System. Press
**Ctrl+,** in the dashboard or open Settings → Workflows → Dashboard pages to enable
Focus, Notifications, Clipboard, Devices, Network, AI usage, Screenshots or Brain.
Extras start off and load only while selected. Settings shows a live preview of
the tab strip: add a page, remove one, or move enabled pages with the up/down
buttons. Core pages can be arranged there too. Both places share the same order.
Number keys 1–9 and 0 select the first ten visible tabs; Ctrl+Tab cycles through
all enabled pages. Tabs share the full header width. Compact tabs show their names
on hover or keyboard focus; the strip scrolls horizontally when necessary.

System CPU process ranking uses recent counter changes among the existing
helper's memory-heavy process candidates. GPU readings depend on driver support;
Intel uses shared memory, while supported AMD/NVIDIA devices show VRAM. Screenshot
delete moves the chosen capture to the trash. AI usage uses the existing
provider-supported reports; Claude's cached limits appear after a response.

Media keeps playback and lyrics together; audio routing is available in Devices.
Browser lyric matching depends on the metadata supplied by the website. Tracks
with plain lyrics show those when timed lyrics are unavailable.

The hourly weather timeline pairs its temperature curve with condition icons and
precipitation chances at labelled times. A percentage is the chance of rain/snow,
not its amount. Storage uses compact Main/Boot filesystem choices when needed.

Focus presets select a duration; **Start focus** begins the timer. Media shows
measured PC audio activity while visible and motion is enabled. The album cover
pulses gently with the selected app’s measured audio levels. Starting playback
in another player automatically follows that player; choosing a player manually
still works. Mixed-output frequency bars are hidden when multiple known local players
are playing. The cover always uses its selected app’s audio level. Without local audio—including phone
playback—the cover stays still and grows, and the audio bars are hidden. Reduced
motion disables the pulse. Volume and connections remain in the right-side
controls. Optional pages stay off by default; Focus can be enabled without adding the other extras to the top strip.

**City photos in Weather**, in Customize or Settings → Workflows, is enabled
by default for new settings; an existing off preference is preserved.
It sends the city name to Wikipedia/Wikimedia, checks the place against the
existing weather coordinates locally, and caches one bounded raster photo for
seven days. The photo credit opens its Commons source. Failed lookups use local
weather illustrations matching the condition and time of day. With city photos
off, these local illustrations need no image downloads. Requests stop when
Weather is hidden.
