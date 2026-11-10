# Translations

Bead UI messages live in `shell/i18n/en.json`, with translator notes in
`shell/i18n/context.json`. Shared helper, launcher metadata, greeter and browser
messages live in `nacre/shell-tools/i18n/{en,context}.json`. No additional real
languages ship yet. The daemon combines its embedded Bead English with the
installed shared catalog and publishes language/catalog updates to Bead.

## Add or change a message

Use a stable dotted ID and preserve the English fallback. Bead views use the
shared UI translation primitive, for example:

```luau
ui.Text{text=ui.tr("bead.settings.title")}
```

Add the English text and translator context to the matching catalog. Helpers
use `nacre_i18n.tr(id, fallback, args)` or the existing Rust `tr` function;
launcher rows pair their English label/description with a `labelId` or
`descriptionId`. The shared settings metadata retains those same stable IDs.
The greeter uses its independent Qt message engine and generated subset.

Translate complete messages. Put paths, user content, numbers and other dynamic
values in named placeholders so translators can reorder them. Plurals use
`one` and `other` forms and a numeric count. Preserve placeholder names and
avoid splitting sentences into separately translated fragments. Commands,
protocol values, app IDs and user-provided labels remain literal data.

## Check English and pseudo

Use an isolated preview or disposable test account, not daily preferences.
Pseudo mode expands and accents messages while protecting placeholders and
arguments. Check long labels, narrow windows, keyboard operation and plural
counts. Keep language selection and support claims aligned with actual runtime
capabilities.

The desktop CI runs the shared coverage/assets checks and the native Bead
translation checks. Relevant commands, in the disposable check environment:

```sh
python tools/i18n/check.py
python tools/i18n/generate.py
python tools/i18n/tests.py
cargo test --locked -j2 -p nacre-i18n
cargo test --locked -j2 -p nacre-shell i18n
```

Shared coverage rejects missing or unused IDs, conflicting English fallbacks
and orphan translator context. Bead's native checker owns its separate catalog.
Remove retired IDs and context together. The generated greeter/browser assets
come from the shared source; regenerate them after changing those messages.
Never test authentication or change a live login configuration for translation
checks.

Translation platforms should import the monolingual language JSON only,
not `context.json`. New languages need matching plural rules and tests before
being offered as supported choices.
