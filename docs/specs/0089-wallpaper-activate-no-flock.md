# N0089 — `wallpaper.activate` must not block the daemon on the publication lock

B0171 (see the Bead spec 0171-wallpaper-picker-live.md) reproduced the owner's "picker does nothing after the first click":
with the daemon at 9b87d913, every `wallpaper.activate` after the first parked the daemon's event loop in
`flock_lock_inode_wait`. `activate_request` took `classic_state::publication_lock` only to write the small
`current-selection-generation` file, but the `wallpaper-media select` child of the previous click holds that lock while it
publishes, and the daemon cannot serve it until it returns.

Fix: write the generation file with `files::atomic` (rename, already atomic) and do not take the lock on the event loop.
The select children still re-check the generation under the lock before they publish (`selection_generation_current`), and
a new selection awaits the aborted previous task, so a superseded child cannot overwrite the newest selection.

Nested result (desktop test host, owner's 17 wallpapers, 5 rapid selections x 3 cycles): before, 2-3 of 10 clicks acknowledged and the
daemon stuck in flock; after, all acknowledged and the daemon idle in epoll. Deploy needs a daemon restart.
