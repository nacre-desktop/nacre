# N0100: launcher gaps (recorder sound, web search engine)

Context: Bead B0199 closes the launcher gaps B0197 left (inline forms, recording indicator,
recording sound, search engine, confirmations). Two of them need a Nacre-side field; the
rest is Bead. The Bead spec is `docs/specs/0197-launcher-actions.md` there.

## Recorder sound (`recorder.start`)
- `RecorderStart` gains an optional `audio`: `none` | `system` | `mixed` (absent = `none`, so every
  existing caller, including the control-centre tile, is unchanged). Regenerated:
  `protocol/schema/recorder-start.json`, `protocol/luau/control-center.d.luau`.
- `none`/absent: unchanged, `menu-backend tool record-start|record-screen`.
- `system`/`mixed`: the daemon starts `launcher-workflows record area|screen` with `{"audio": ...}` on stdin,
  the helper path QML's recording form used (virtual capture sink for `mixed`, cleaned up by
  `record-stop`). The capture lane (one interactive tool at a time) and the "already recording"
  refusal apply to both paths. An unknown value is a validation error before anything runs.

## Web search engine (`search-web` activation)
- The row the daemon hands the UI on activation (`ui.menu.activate`, `uiRow`) now carries `webUrl` for
  `search-web`: the URL for the owner's `webEngine` source option (`duckduckgo`/`google`/`brave`) and the
  typed query, the same URL QML built with `Sources.webUrl(sourceOptions.webEngine, ...)`. Query rows
  are untouched, so the frozen QML references stay byte-identical. A UI without the source options
  (Bead) opens `webUrl`. Omitted over 8192 bytes (schema allows 65536); the UI then uses its own fallback.

## Tests
- `recorder_start_with_sound_uses_the_launcher_recorder` (daemon fixture: stdin document, the launcher
  recorder command, plain path for `none`, validation for an unknown sound).
- `search_web_activation_carries_the_owners_engine` (daemon menu engine, brave).

## No temporary duplicates
None added.
