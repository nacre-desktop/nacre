# Security and privacy review

Reviewed 2026-10-13 against `main` in an isolated worktree. This is a source review
with disposable regression tests, not a penetration test or a live account audit.
No HIGH-severity Python issue was confirmed. Runtime Python and frozen parity
fixtures were left unchanged; follow-ups are in [Rust migration](rust-migration.md#fix-during-port).

| Severity | Area | File | Finding / status |
| --- | --- | --- | --- |
| Medium | Shell / IPC | `hypr/scripts/window-trash.sh` | **fixed**: cached state was sourced as shell code; now validated JSON, validated IDs, link refusal, atomic 0600 files and 0700 directory. Old 30-second shell records are ignored; existing trashed windows remain manually closable. |
| Medium | Notification privacy | `nacre/shell/modules/notifications/NacreNotice.qml` | **fixed**: image/icon URLs could initiate network fetches; accept local files/resources/providers and named icons only. |
| Medium | Credentials | `bin/codex`, `ai/remote-codex-wrapper.sh` | **fixed**: wrappers inherited permissive umasks; new provider files/directories now inherit 0600/0700 restrictions. Existing files are not repaired. |
| Medium | Brain availability | `brain/control.py` | **fix during port**: no connection timeout or concurrent-client cap; malformed non-object action JSON raises an uncaught error. Existing 4 KiB action limit does not bound slow connections or note responses. |
| Low | Brain auth lifetime | `brain/auth.py` | **fix during port**: bootstrap remains reusable until restart; cookie persists across restarts without server-side expiry/revocation UI. Auth files are 0600 and directory 0700; no HTTP request logging. |
| Medium | Screenshots / location | `nacre/shell-tools/screenshot.py`, `timezone.py` | **fix during port**: full screenshots inherit umask; system timezone status exposes city/accuracy at 0644. Precise device coordinates are ephemeral. |
| Medium | History retention | `notification-history.py`, `desktop-extras.py`, `composer.py`, `sidebar-chat.py` under `nacre/shell-tools/` | **fix during port**: notifications, chat/drafts/screenshots lack age expiry; clipboard thumbnail cleanup occurs only on refresh. Private-file protections exist but old directory modes are not consistently repaired. |
| Medium | Optional downloads | `brain/provision-semantic.py`, `brain/semantic.py`, `bin/nacre-ai-skills`, `tools/fetch-wallpapers.py` | **fix during port**: model revision/content and archive/image digests not locked; some downloads/extractions are unbounded. Skills/images pin source commits; Python top-level packages pin versions but transitive installs lack a hash lock. |
| Low | Local IPC | `nacre/shell-tools/sidebar-chat.py` | **fix during port**: socket is 0600 inside 0700 state; client lines and client count are unbounded. Helper-service stdin already has request, type and tool limits. |
| Medium | Background network | `NacreWeather.qml`, `NacreMediaDetails.qml`, `lock-info.py` | **accepted risk**: weather uses IP lookup when no city is set; lyrics/artwork can disclose listening activity. Inventory below documents current triggers; defaults/opt-in semantics unchanged. |
| Medium | AI / updates | `profiles/`, `nacre/shell-tools/updates.sh` | **accepted risk**: authorized full-access agents can read user data and run commands; `paru -Syu --skipreview` executes AUR build scripts without the review screen. Owner-selected policy unchanged. |
| Low | Privileged installation | `TimezoneSettings.qml`, `timezone.py`, `login-install.py` | **accepted risk**: initial setup authorizes user-supplied Python/theme source as administrator. No dedicated timezone polkit policy is shipped; generic `pkexec` challenges apply. Installed helper uses fixed command arrays, validated IANA zones/actions and root checks for mutations. |
| Low | User-session trust | `nacre/shell/modules/NacreShellIpc.qml`, `AppLaunch.qml`, `compositor_backends/niri_config.py` | **accepted risk**: session IPC is accessible to the session user; application launch commands in bindings are intentional shell configuration. Arguments derived from shortcut/workspace data are quoted; QML launches use argv arrays. This is not an isolation boundary against a compromised application running as that user. |
| Low | Provider secrets | `bin/nacre-ai-account`, `bin/nacre-ai-claude`, `ai/install.py` | **accepted risk**: provider CLI owns token/keyring storage; Nacre does not force a storage backend. Named Codex homes are 0700; upstream default/Claude homes and pre-existing credential modes require a separate local audit. API-key environment variables are inherited; same-user processes can inspect them. |

## Existing boundaries checked

Brain serves on `127.0.0.1`; Host validation rejects other origins/DNS-rebinding
names. Private API reads require a constant-time checked cookie. Actions require
the cookie, exact loopback Origin, JSON content type and allowlisted names.
Resolved note paths stay inside the Markdown vault and exclude hidden paths;
static assets use a fixed allowlist. CSP, no-store and nosniff headers apply.
The bootstrap browser command contains a private local HTML path, not a token;
the token travels in the local HTTP URL. The HTTP logger is disabled. Another
process with the same user privileges can read the credential files/browser
profile; loopback authentication does not protect against that process.

Clipboard storage uses `wl-paste --watch cliphist store`. Upstream
[wl-clipboard](https://github.com/bugaevc/wl-clipboard/blob/master/src/wl-paste.c)
passes sensitivity hints and [cliphist](https://github.com/sentriz/cliphist/blob/master/cliphist.go)
skips `CLIPBOARD_STATE=sensitive`; this depends on password managers/compositor
actually supplying the hint. It cannot recognize every password. Cliphist's
upstream default caps history at 750 entries, not a time period; user configuration
may override it. Nacre shows up to 160 recent entries plus pins. Notifications
exclude transient items and selected desktop feedback, but have no age/count
retention policy on save. Chat history is intentionally persistent. Deletion is
not secure erasure and does not clear backups or provider-side history.

Codex/Obsidian binaries and font assets verify pinned SHA-256 digests before
use. Orient and packaged Git sources pin commits; VCS checksum `SKIP` is not a
signature check. npm uses its integrity lock and `--ignore-scripts`; reviewed CI
actions pin commits. No curl-to-shell installation was found in repository scripts.
The normal installer stages/deploys user files; package-manager installation,
timezone setup and the login-theme installer are the explicit privileged paths.
Wi-Fi authentication uses `nmcli --ask`, keeping the password out of Nacre argv.
Diagnostics has secret/path/network redaction, but exported reports still require
review. No real credentials, live histories or live configuration were read.

## Outbound-call inventory

Every HTTP request exposes the public IP and request timing. This inventory covers
repository-owned calls and delegated network-capable tools; destinations chosen
by browsers, provider CLIs, GeoClue or package managers depend on their configuration.

| Caller / destination | Data sent and trigger | Consent / integrity |
| --- | --- | --- |
| Weather: `ipinfo.io/json` | IP lookup at shell weather startup/30-minute refresh when city is blank. | Automatic existing behavior; set a city to avoid this lookup. 8-second request timeout, 512 KiB response cap. |
| Weather: `geocoding-api.open-meteo.com/v1/search` | Configured/detected city, language; when city coordinates are not cached. | Existing weather behavior. |
| Weather: `api.open-meteo.com/v1/forecast`, `air-quality-api.open-meteo.com/v1/air-quality` | City latitude/longitude, requested fields and timezone; weather refresh. | Existing weather behavior; cached location/forecast file is 0600. |
| Media: `lrclib.net/api/get` | Track, artist, album and duration when media details update with lyrics enabled. | Dashboard lyrics setting, currently enabled by default; 6-second timeout and 300 KiB response cap. |
| Media: player-supplied artwork URLs | Artwork URL via QML media images, dashboard album palette and lock artwork. | Rendering/settings dependent; may disclose listening activity. Python fetches bound size/time and require HTTPS, including final redirect scheme. QML image loading does not share those Python checks. |
| Notifications | Previously sender-controlled image URLs on expansion/icon rendering. | Fixed: renderer prevents network image sources. |
| Launcher: `aur.archlinux.org/rpc/v5/search/` | Package search text when explicitly searching packages. | User search; 3-second timeout. |
| Updates: `checkupdates`, `paru`/`yay`, `pacman`, `flatpak` | Repository metadata, package inventory/version queries and selected package downloads. | Status refresh may contact configured mirrors/AUR; actual upgrades prompt explicitly. Package manager verifies distribution packages according to its configuration; AUR build scripts retain owner-selected skipreview. |
| Browser routes | Explicit Google search text, mail URL, arbitrary selected web links; welcome project/docs/source/license links. | User actions; destination browser owns subsequent network activity. |
| AI provider CLIs | Prompts, selected attachments, conversation/tool context, account authentication/usage requests. | Optional AI installation/sign-in and explicit chat actions; full access follows selected profile. Providers own endpoint/storage behavior. |
| Brain synchronization / remote workers | Markdown/text/Base/Canvas notes and baseline over configured SSH peer; remote task metadata/commands. | Configured host; enabled Brain sync timer runs every two minutes. SSH host-key/account policy applies; no auth copying. |
| Brain semantic provisioning | PyPI packages and `BAAI/bge-small-en-v1.5` model assets via FastEmbed's configured sources. | Explicit `brain/provision-semantic.py`; normal embedding is local-only with Hugging Face offline/telemetry-disable flags. No document upload by embedding helper. Model revision/hash lock deferred. |
| AI installers | Official Codex/Obsidian assets from GitHub; Claude SDK from PyPI. | Explicit installers; binary SHA-256 pins, SDK version pin; transitive Python hash lock deferred. |
| Shared skills installer | Selected archives from `codeload.github.com` at recorded commits. | Explicit skill install; path traversal rejected, archive digests/size limits deferred. |
| Font setup / translation audit | Selected font/license URLs at `raw.githubusercontent.com`; translation audit retrieves manifest fonts. | Setup/audit commands; source commits, byte sizes and SHA-256 pins. |
| Curated wallpaper fetch | Curated image URLs at `raw.githubusercontent.com` at a fixed commit. | Explicit developer tool; image hashes/response limits deferred. |
| Builds / packaging / CI | Git submodules and pinned niri checkout, Cargo crates, pip/npm packages, distribution repositories/container images, GitHub checks/artifacts/releases. | Explicit builds/workflows; Git/Cargo/npm pins, package verification; rolling OS/container tags and Python transitive locks remain supply-chain risks. Release workflows require explicit dispatch; this review creates no release/deployment. |
| GeoClue | System-configured location backend may send location/network observations. | Automatic timezone setup requires administrator authorization; only fresh sufficiently accurate non-GeoIP fixes change timezone. GeoClue backend traffic is delegated, not an offline guarantee. |

## Validation and remaining decisions

Regression tests cover JSON window-state round trips, malicious/linked/traversal
records, private provider file creation, actual notification URL filtering, and
Brain auth/CSRF/path/file-serving/length boundaries. All use disposable state;
HTTP tests bind ephemeral loopback ports. Frozen helper fixtures are replayed,
not regenerated. No live desktop, installer apply, root operation, external
probe, release or deployment is part of validation.

Decisions for the owner: choose retention periods for notifications, clipboard,
draft screenshots and chats; decide whether weather IP lookup and default lyric
lookup need separate consent controls; choose a required provider credential
backend if upstream defaults are insufficient. Until those decisions, preserve
current behavior and implement the scoped hardening during the Rust port.
