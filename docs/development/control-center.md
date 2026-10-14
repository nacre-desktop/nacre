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
