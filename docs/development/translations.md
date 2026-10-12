# Translations

Nacre's English messages live in `nacre/shell/i18n/en.json`. No additional real
languages ship yet. `context.json` contains translator notes; `options.json`
connects stable enum values to display-message IDs. These two files are metadata,
not language files.

## Add or change a message

Use a stable, dotted ID and preserve the existing English wording:

```qml
text: NacreI18n.tr("launcher.search.placeholder", "Search everything…")
```

Add the same ID and English text to `en.json`, with a short note in `context.json`.
Keep the ID when wording changes; update the English source and fallback together.
Give ambiguous words context: "Open" on a button is a verb, whereas a window state
may need a different translation. Do not reuse an ID merely because its current
English happens to match.

Translate complete messages. Put dynamic values in named placeholders:

```qml
text: NacreI18n.tr("example.workspace", "Workspace {workspace}", {
    workspace: workspaceNumber
})
```

For counts, use `one` and `other` forms and pass the numeric count:

```json
"example.windows": {
  "one": "{count} window",
  "other": "{count} windows"
}
```

```qml
text: NacreI18n.tr("example.windows", {
    one: "{count} window", other: "{count} windows"
}, {count: windowCount})
```

Do not assemble sentences from separately translated fragments. Translators must
be able to reorder placeholders. User text, paths, URLs, application-provided
content, key sequences and file-filter patterns stay in arguments. Pseudo mode
preserves placeholders and HTML tags; arguments are substituted afterward.

Use `NacreI18n.number`, `date`, `time` and `dateTime` for displayed values. Keep
machine formats, hexadecimal colors, timestamps and command arguments separate.
The locale controls presentation; a missing language or message falls back to the
English source. English and pseudo currently use the English cardinal plural rule.
A new language with different plural categories needs a matching rule and tests
before it can be offered as a supported Language choice.

## Catalogs and helpers

Launcher rows retain their stable command ID and English fallback. Add `labelId`,
`descriptionId` and, where applicable, `confirmId`. Built-in catalogs require these
companions. Settings and Welcome catalogs similarly pair visible fields with IDs.
Their presentation layer localizes copies; command values remain unchanged.
User extensions may keep custom labels. Overriding a built-in English field drops
its inherited message ID unless the extension explicitly supplies a replacement.

Standalone Python desktop helpers use `nacre_i18n.tr(id, fallback, args)`. They read
the same English catalog. The small CLI bridge supports notification messages in
compositor shell scripts. Keep notification sender identities and compositor
window identifiers stable; localize the captions that Nacre renders for users.
Native window rules and panel ownership checks must not depend on a translation.

The login greeter uses the same message engine without requiring Quickshell.
Its generated `messages.js` subset comes from the English source. Its developer
preview accepts `configuration.language: "pseudo"`; installable theme assets
include the engine and generated messages. Never test authentication or change
someone's live login configuration to check translations.

## Check English and pseudo

Language offers **System default** and **English**. Pseudo appears in the Language
setting only when `NACRE_DEVELOPER_MODE=1`; it can also be selected directly with
`NACRE_LANGUAGE=pseudo`. The environment override wins over the saved preference.
Use an isolated preview or test account, not the maintainer's daily preferences.

Pseudo accents letters, expands messages by roughly 40% and surrounds them with
brackets. Inspect the launcher and its forms, all Settings pages, Colors, Welcome,
the dashboard, control popups, tooltips and notifications. Check long labels,
small windows, keyboard operation, placeholders and plural counts. External app
names, user content, icon names and protocol data are not translated.

```sh
python3 tools/i18n/check.py
python3 tools/i18n/tests.py
python3 tools/check.py
```

CI rejects unwrapped owned QML/catalog literals and missing/unused English IDs.
Native tests exercise the message engine, locale formatting, English fallback,
pseudo protection and existing English interactions. New UI needs a message ID and
a translation test alongside its behavior tests.

## Translation platform setup

Use **monolingual JSON**, with `en.json` as the source-language file. Weblate and
Crowdin can edit the ID-to-text JSON; nested `one`/`other` entries are the plural
message forms. Import only language JSON files, not `context.json` or `options.json`.
Supply translator context from `context.json`, preserve placeholder names and
validate the result with the coverage checks before adding a language.

Qt's locale-aware date and number APIs are documented in the
[Date](https://doc.qt.io/qt-6/qml-qtqml-date.html) and
[Locale](https://doc.qt.io/qt-6/qml-qtqml-locale.html) references. Weblate documents
[monolingual source files and formats](https://docs.weblate.org/en/latest/formats.html).
