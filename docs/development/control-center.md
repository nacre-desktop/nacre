# Shared control-center editor

`NacreControlEdit.qml` owns the catalog, supported-device filtering and layout
actions. Settings can reuse it without duplicating preferences or the panel editor:

```qml
NacreControlEdit {
    screen: window.screen
    section: "layout"
    settingsRows: true
    showHeadings: false
    showHint: false
    showReset: false
}
```

`section` accepts `all` (the panel default), `layout`, `detected`, or `available`.
The last two expose detected microphone/keyboard sliders and other unselected
controls. `settingsRows` uses full-width rows, shared size-scale tokens and right
aligned add/reorder/remove icons. Headings, hint and reset default to visible;
Settings can supply one surrounding heading. Inherited disabling dims rows and
blocks their actions. Keyboard focus follows mutations within the displayed
section. All instances share `NacreControlPreferences`; readonly `selectedIds`,
`detectedEntries` and `availableEntries` expose supported entries to the host.

`lipHoverDelay` is an integer desktop preference from 0 to 800 milliseconds,
defaulting to 350. It affects the top, left and right frame lips only; zero opens
immediately. Status-icon and calendar hover delays remain independent.

The QML preferences service applies defaults without changing the frozen helper
registry. The complete catalog is selected by default. Untouched previous defaults
upgrade to the complete layout; custom layouts, empty layouts and explicit removals
stay intact. Legacy custom layouts gain microphone and keyboard sliders beside
existing output/display sliders. Saves carry `defaultsRevision` (currently 2).
Unsupported devices stay selected in preferences but are hidden until available.
The overview uses full-width sliders and appearance, paired settings, and rows of
three compact tools. Every row shares its width evenly, including after capability
filtering or custom reordering. VPN navigates to
profile selection rather than guessing a connection.
