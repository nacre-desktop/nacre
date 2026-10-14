# Synthetic support-report references

These 26 new `diagnostics/support-*` cases capture the current Python collector,
renderers and transitive maintenance facts at Nacre `01769f3f`. Source SHA-256
values are recorded in each case and checked by `freeze_diagnostics.py`:

- `diagnostics.py`: `0176fe8eca21315c075f1aabbe6d3be48dd20fadf594fc8f0c0fa73b08c3a9fd`
- `maintenance.py`: `ef0a0d85ea966a69a3cb5f83c53cbef6e2c89a31c1175c143f2ea53fe77a0ae3`

The developer adapter executes only inside the existing Bubblewrap harness.
System directories, HOME and probes are synthetic; platform values, identities
and clock are controlled inputs. Every known subprocess response is scripted,
and an unknown probe fails rather than invoking a live executable. Calls retain
their arguments, timeout, stdin, locale and no-color requirements. The existing
native GPU inventory bridge is supplied as a controlled dependency result; its
own filesystem behavior is covered by the resource-helper tests.

The current implementations of `collect`, `redact`, `markdown`, `support_report`,
`maintenance_facts`, `maintenance.state`, `configuration_errors` and the actual
`maintenance.main` doctor argument parser execute unchanged. Only dependency
boundaries are substituted. Neither production hooks nor a second runtime
implementation are added. All 26 cases write or delete zero files. Existing
golden files are unchanged, and capture refuses changed sources or existing
destinations.

The `stdout.value` field is the original return value; `stdout.calls` records
dependency interactions. JSON report contents are compared as parsed values.
Markdown and the complete doctor JSON/Markdown output are preserved as strings,
including escaping, ordering and trailing newlines. Fixed time avoids output
normalization. The maintenance-state input is an internal intermediate and may
contain synthetic profile fields; those fields must be absent from exported
maintenance facts and support reports.

Retained assertion mapping in `test_diagnostics_reference.py`:

| Historical diagnostics test | Synthetic evidence |
| --- | --- |
| `absence_and_mixed_scale_inventory_fixtures` | Both original hardware fixtures, exact flags/scale/refresh values; unavailable report with empty hardware |
| `recursive_redaction_covers_values_keys_and_free_form_errors` | `redact-full`, nested keys/values, IP/MAC/URL/home/name/secret removal; public numeric metrics unchanged |
| `short_named_credentials_in_logs_are_redacted` | `redact-short` |
| `probes_are_bounded_read_only_and_do_not_return_sensitive_stderr` | Probe success/missing/timeout/output-limit cases; exact deadline/stdin/locale/no-color and withheld stderr |
| `optional_missing_unit_preserves_other_service_evidence` | Partial systemctl stdout retained, ordinary failed command stdout withheld |
| `report_uses_backend_diagnostic_commands_and_identity` | Fixture backend commands recorded; selected version/display names preserved |
| `report_has_all_sections_and_redacts_before_either_renderer` | Complete collector, Markdown and both support-report outputs; privacy checks and bounded read-only query arguments |
| `malformed_device_payloads_cannot_break_the_export` | Malformed audio/monitor payloads, NaN/bool/type rejection, non-list monitor input |
| `unavailable_commands_do_not_prevent_a_report` | Unavailable collector, absent packages/devices and unavailable values |
| `doctor_cli_is_one_valid_json_document_or_markdown` | Actual doctor parser with both flags inside the synthetic boundary; no real collector is run |

The maintenance assertions in `test_compositor_helper_migration` are also
retained: selected backend version command and configuration error/timeout policy.
Additional cases preserve partial service evidence, on-demand portals, managed
file drift, update age and corrupt-state fallback. Native transport equivalences
must be declared separately; the references are not normalized to a future port.
