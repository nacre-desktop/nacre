# Hosted helper assertion mapping

Reference: the Python helpers and tests at Nacre `488fa6d1` (before deletion).
New `api-*` references were captured from those Python sources in the existing
isolated harness; each case records the source SHA-256. Existing golden files
were not regenerated. `examples/parity-api.rs` calls the native library directly
and is a developer test adapter, not an installed helper.

## Helper service

All ten historical `test_helper_service.HelperServiceTests` methods are retained
by these native checks; the Python suite may be retired with its implementation:

| Historical method | Native evidence |
| --- | --- |
| `renderer_worker_uses_managed_palette_interpreter` | `test_helper_transport.renderer_worker_uses_central_native_helper`; intended command change to the one central native path |
| `media_art_service_matches_cli_with_candidate_runtime_without_pythonpath` | `tests/media.rs::media_art_service_matches_cli_without_python_or_external_orient`, including PNG/cache bytes, roles and ready packet |
| `pending_limit_rejects_excess_work_and_output_is_bounded` | `tests/service.rs::global_pending_bound_and_duplicate_ids_match_protocol` and `response_limit_returns_the_existing_error_envelope`; frozen `bounded-queue` / `oversized-response` |
| `network_dispatch_preserves_read_only_snapshot_and_failure` | `tests/network.rs` CLI/service private-bus success, failure and deadline tests |
| `bad_packets_and_exception_fail_only_their_request` | `operation_errors_do_not_break_fifo_or_eof_drain`; frozen `ids-and-invalid` / `hosted-crash` |
| `nonfinite_oversized_and_invalid_arguments_are_rejected` | `validates_ids_types_bounds_and_unicode_lengths` / `bounded_frames_and_invalid_json_match_python_errors`; frozen `ids-and-invalid` / `oversized-request` |
| `queue_is_bounded_and_independent_requests_can_complete_out_of_order` | `global_pending_bound_and_duplicate_ids_match_protocol` / `independent_lanes_do_not_wait_for_a_busy_helper`; frozen `bounded-queue` / `independent-lanes` |
| `same_tool_burst_is_fifo_without_starving_independent_helper` | `independent_lanes_do_not_wait_for_a_busy_helper`: four serial requests, independent fifth finishes first, start barrier and exact FIFO order |
| `explicit_dispatch_cannot_launch_a_menu_tool` | frozen `disallowed-menu-actions`: sequential declared side-effect-free refusals |
| `cli_and_service_results_match_and_modules_are_reused` | `tests/hosted_cli.rs::cli_and_service_results_match_across_repeated_native_operations`: same PID, seven real operations, empty PATH |

The actual QML kill/malformed/timeout recovery and no-replay assertions remain
in `test_helper_transport.py`. Its stalled provider is a compiled developer-only
`service-fixture` using production protocol code; installed helper builds cannot
load that provider. Log markers explicitly enable `qml.debug` rather than depend
on inherited host logging. No fault-injection mode is added to the installed bin.

`crates/nacre-helper/tests/hosted_assertions.rs` retains these test methods. Rust
test names are the original method names without the `test_` prefix:

| Historical suite | Methods retained |
| --- | --- |
| `test_launcher_preferences.LauncherPreferencesTests` | `favorites_and_hidden_updates_preserve_other_choices`, `action_favorites_roundtrip_alongside_apps`, `invalid_preference_is_rejected_without_writes`, `malformed_existing_preferences_are_preserved`, `recent_ids_are_bounded_without_reordering_favorites` |
| `test_notification_history.HistoryTests` | `private_roundtrip_keeps_only_display_fields_and_clear_is_durable`, `feedback_is_removed_but_messages_and_completion_are_kept`, `invalid_input_cannot_overwrite_existing_history` |
| `test_pearl_dashboard_data.PearlDashboardDataTests` | `tabs_and_preferences_reject_invalid_values`, `private_writes_and_focus_deadline_survive_pause_and_resume`, `local_calendar_unfolds_titles_dates_and_excludes_cancelled`, `lrc_orders_multiple_timestamps_and_ignores_tags`, `cached_media_does_not_fetch_or_publish` |

Environment-dependent assertions run in separate child test processes with
disposable HOME directories; the parallel test runner's environment is never
mutated. Permissions, field removal, ordering, uniqueness, usage counts, focus
state transitions, calendar size limits and parsed text are checked explicitly.
The existing black-box media `fresh-cache` case additionally proves a cache hit
has no fetch or publication side effects in a network-isolated sandbox.
Dashboard dict/list tab errors are additionally compared through both CLI and
module-operation `*-unhashable-*-tab` references: exact Python 3.14 message and
TypeError category are preserved, rather than only asserting that an error occurs.

The media portion of
`test_helper_service.test_media_art_service_matches_cli_with_candidate_runtime_without_pythonpath`
is retained in `tests/media.rs::media_art_service_matches_cli_without_python_or_external_orient`:
the same two-pixel PPM input, five roles, version-1 ready packet and exact CLI/service
results, plus identical PNG/JSON cache bytes and 0600 modes. PATH is empty to prove
that the native helper needs neither Python nor an external Orient executable.
Interpreter/Pillow/SDK installation assertions belong to the retained Python
palette runtime tests, rather than to the migrated media helper.

An end-to-end comparison with the unchanged Python media helper covered all seven
Orient gallery images plus the tiny PPM, in both modes: 16 cases, zero changes
across 80 palette roles, identical media cache bytes and modes. The direct library
caller preserves the separate `~/.cache/orient/palettes` entry written by the
Orient CLI. The same 16-case comparison verifies compact payload bytes and 0600
mode, and independently computes each cache filename from that run's actual
poster stat, settings and engine/decoder identity. No cache hashes are normalized.
`tests/media.rs` checks that valid existing entries return unchanged, while
malformed JSON, wrong engine/version/name/input and invalid palettes recompute
and publish the exact payload. Prior cache entries remain untouched. Cache I/O
belongs to Nacre's boundary, keeping Orient's core pure.

The weather assertions in the mixed dashboard suite remain Python tests because
weather is outside this cutover. The system ownership assertion is mapped by
the additional-helper contract metadata. Menu assertions require their own
completed mapping before the historical suite
is removed. Network and helper-service mappings are detailed below.

Network discovery now reads typed NetworkManager properties instead of parsing
nmcli's escaped text. These black-box cases retain the observable assertions:

| Historical `test_network_state` method | Native fixture evidence |
| --- | --- |
| `escaped_colon_backslash_unicode_and_empty_fields` | `legacy-ssid-escaping-hidden`: literal colon, backslash, Unicode and hidden SSID; malformed nmcli escape syntax is a retired transport detail |
| `ap_records_keep_active_signal_and_hidden_network` | `legacy-ssid-escaping-hidden`: active flag, exact signal/frequency and hidden SSID |
| `invalid_and_truncated_ap_records_are_ignored` | `invalid-ap-rows`, `legacy-invalid-interface`, `legacy-ssid-escaping-hidden`: bad BSSID, out-of-range signal, invalid frequency/interface and duplicate removal; truncated text rows are a retired transport detail |
| `nmcli_space_marker_keeps_available_networks_and_ssid_whitespace` | `legacy-ssid-whitespace`: four exact SSIDs and active flags; nmcli marker syntax is replaced by the typed active-AP property |
| `interface_requires_connected_wifi_and_valid_name` | `legacy-invalid-interface`, `legacy-disconnected-interface`, `connected-escaped`, `no-wifi` |
| `disabled_radio_and_failures_are_reported_without_sensitive_output` | `radio-off`, `unknown-radio`, `command-failure`, `missing-command` |
| `ethernet_only_does_not_query_wifi_points_or_offer_radio` | `no-wifi`: exact properties/side effects; any unexpected AP query fails |

`tests/network.rs::stalled_private_networkmanager_respects_deadline` reaches the
real eight-second method deadline on a private peer. Its other controls reject
oversized malformed scalars, correctly typed interface strings and correctly
typed GetDevices arrays, while keeping sensitive contents out of stdout/stderr.
Small malformed values still produce the original unknown-radio error. These
retain `snapshot_commands_have_deadlines_and_never_scan_or_query_secrets` and
`oversized_snapshot_is_rejected`; the frozen property fixtures also fail any
unexpected scan/secret query. The size bound now applies to the 2 MiB raw typed
D-Bus reply body, replacing the old decoded nmcli stdout-character bound.

Service protocol assertions retain their original packets and ordering through
the following library tests and unchanged black-box dialogues:

| Historical `test_helper_service` method | Native evidence |
| --- | --- |
| `pending_limit_rejects_excess_work_and_output_is_bounded` | `tests/service.rs::global_pending_bound_and_duplicate_ids_match_protocol`, `response_limit_returns_the_existing_error_envelope`; `bounded-queue`, `oversized-response` dialogues |
| `bad_packets_and_exception_fail_only_their_request` | `operation_errors_do_not_break_fifo_or_eof_drain`, `bounded_frames_and_invalid_json_match_python_errors`; `ids-and-invalid`, `hosted-crash` dialogues |
| `nonfinite_oversized_and_invalid_arguments_are_rejected` | `validates_ids_types_bounds_and_unicode_lengths`, `bounded_frames_and_invalid_json_match_python_errors`; `oversized-request` dialogue |
| `queue_is_bounded_and_independent_requests_can_complete_out_of_order` | `independent_lanes_do_not_wait_for_a_busy_helper`, `global_pending_bound_and_duplicate_ids_match_protocol`; `independent-lanes`, `bounded-queue` dialogues |
| `same_tool_burst_is_fifo_without_starving_independent_helper` | `independent_lanes_do_not_wait_for_a_busy_helper`: exact independent-first/FIFO replies, no output sorting |
| `explicit_dispatch_cannot_launch_a_menu_tool` | `disallowed-menu-actions` dialogue and the existing `all-hosted-operations` denied-action request |
| `cli_and_service_results_match_and_modules_are_reused` | `tests/hosted_cli.rs::cli_and_service_results_match_across_repeated_native_operations`: the same seven operations, one persistent PID, exact parsed CLI/service results, durable launcher file and EOF exit |

Network dispatch success/failure is tested against a private typed manager in
`tests/network.rs`, with CLI/service results compared at the production entry
points. A mock closure alone proves framing, rather than provider selection.
The non-migrated renderer-runtime assertions must still be accounted for before
the whole historical service suite is removed.

Run:

```sh
cargo test -p nacre-helper --test hosted_assertions
cargo build -p nacre-helper --examples
python tests/parity/helpers/run.py --case '*/api-*'
```

The last command requires the default native registry wiring at cutover. The
adapter can also be selected explicitly with `--command
'target/debug/examples/parity-api {name}'`. Keep the frozen parsed JSON, file
bytes, modes, deletions and effects comparisons strict.
