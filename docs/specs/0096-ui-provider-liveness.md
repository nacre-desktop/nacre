# N0096: UI provider liveness and newest-wins registration

Context: Bead B0190. A Bead shell whose link thread deadlocked kept its daemon
socket open, so the daemon routed every ui.dispatch to it and each call timed
out after 3 s; nothing recovered until the shell was restarted by hand. The
shell-side deadlock is fixed in Bead (spec 0190). This unit makes the daemon
recover from any provider that stops answering and from overlapping shells.

## Behaviour (crates/nacre-helper/src/daemon/hub.rs)

- Newest registration wins. `register` from a second client no longer returns
  `Busy "UI provider is already registered"`: the old provider loses the role,
  its in-flight routes are dropped (waiters fail fast, late replies are refused)
  and notifications are told it disconnected. Its socket stays open, so an old
  shell cannot re-register in a loop. When the old shell later exits, the new
  registration is untouched. During a shell switch ui.dispatch follows the new
  shell immediately, never a stale provider id.
- Dead-provider detection. A ui.dispatch route that reaches its own deadline
  without a reply counts as a miss; any reply from the provider resets the
  count; a caller that cancels before the deadline does not count. After 3
  consecutive misses the daemon logs `UI provider stopped answering` and closes
  that client (`remove_client`), so a healthy-but-wedged link sees EOF,
  reconnects and registers again.

Trade-off: an action handler that legitimately overruns its deadline three
times in a row causes one reconnect of the shell link. Any second process that
registers as UI provider (`--ui-provider`, QML `NacreDaemon.registerUi`) on the
same daemon now takes routing from the first; previews must not register.

## Tests

- `hub::tests::oversized_state_and_duplicate_calls_are_rejected_and_newest_ui_provider_wins`
  (old/new shell overlap: routing moves, stale route and reply refused, old exit
  keeps the new provider).
- `hub::tests::provider_missing_consecutive_deadlines_is_closed_and_can_reregister`.

No temporary duplicates.
