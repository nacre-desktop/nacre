# N0024 — Brain web server as an on-demand worker

- Status: Implemented; pending lead review
- Dependency: N0020 native `brain_data`
- Risk: high (loopback HTTP, browser credentials and handoff); size L

Native `brain-control`, `brain-auth`, `brain-discovery` and `brain-launch`
replace the Python control/auth/discovery execution paths. Preserve URLs, routes,
JSON shapes, search, graph, action allowlist, browser profile, workspace/window
handling, theme CSS and the optional `semantic-client` worker contract. Keep
`brain/web` assets unchanged. No new UI features or daemon hosting (N0030).

The standalone server starts on demand and exits after inactivity. Preserve the
current `watch-brain` unit contract first: readiness without reopening a browser.
Do not change installed units or introduce daemon activation. If window rules
require residency, stop and ask the owner before retaining a small supervisor.

## Phases

1. Contract/spec and queue checkpoint.
2. Native discovery, bounded HTTP/auth, graph/search/actions and launcher; native
   command dispatch, wrappers and fixture-only historical/security tests.
3. Review, security/parity corrections and exact-source full desktop CI evidence.
Each phase has a separate unpublished `[no-publish]` commit.

## Acceptance

- Replay frozen `brain-control`, `brain-auth`, `brain-discovery`, `brain-launch`
  cases with `run.py --command 'nacre-helper {name}'`; never regenerate references.
- Retain assertions from `test_security_boundaries.py`, `test_index.py`,
  `test_auth.py`, `test_discovery.py` as Rust tests. Compare discovery labels and
  clusters exactly using public fixture notes. Never read real vaults in tests.
- Loopback binding only, ephemeral test ports, client cap, absolute connection,
  header/body/write deadlines, reject transfer encoding and ambiguous lengths,
  reject non-object JSON, bound responses, bootstrap expiry/revocation, cookie
  expiry, origin-bound browser access and cross-port navigation rejection.
- Reject symlinked state ancestors; preserve CSRF, Origin and Host checks or
  document each tightening as an intended difference with a new regression.
- Full `.github/workflows/check.yml` desktop job in an isolated Arch container
  on the remote CI host; private browser checks only on the desktop with its lock.

## Boundaries

No publication, push, live desktop/service/configuration/keybinding/unit changes,
private brain writes, or local builds/tests. Use nice 10, Cargo `-j2`, fixture
vaults, private sessions and approved Git identity for author and committer.
Existing AI permissions and `paru -Syu --skipreview` stay unchanged.

## Intended security differences

- Host-wide browser cookies are insufficient authorization. A separate random
  credential is stored in browser localStorage, which is scoped to the HTTP
  origin including its port. Bootstrap still accepts `/?token=...&handoff=0|1`
  and responds with 303 plus HttpOnly/SameSite=Strict cookie; its Location adds
  an authentication fragment. A generated prefix on the served app/close scripts
  stores that fragment, removes it from history, and attaches the credential to
  same-origin fetches only. Candidate fragments are validated through an
  authenticated same-origin endpoint before replacing a saved credential.
  The API bearer never appears in static assets or cookies.
  Protected API requests require both credentials; cross-port navigation and a
  stolen-cookie replay with forged Fetch Metadata must fail.
- Before a browser handoff, native launchers verify a fresh nonce HMAC from the
  server against private session state. The optional health proof header leaves
  the public health JSON unchanged and rejects an impostor occupying an idle port.
- Protected GET APIs additionally require `Sec-Fetch-Site: same-origin` and
  reject foreign Origin/Referer. Non-browser clients must supply that metadata
  and the private API credential. POST retains exact canonical Origin, cookie
  and application/json checks. The canonical origin remains `127.0.0.1`.
- Bootstrap expires after five minutes and is consumed once. Explicit launches
  refresh its private handoff file. Restart revokes old bootstraps. Sessions
  expire after 30 days server-side and via cookie Max-Age; each listener port
  has separate session/marker state under `auth/ports` (this does not port-scope
  browser cookies). Legacy sessions reauthenticate through the existing handoff.
  A page already loaded from the Python server needs one reload to load the
  origin-bound fetch hook; subsequent native-client restarts retain credentials.
- At most 32 clients; absolute 60-second connection lifetime, five-second
  request parse/body and response-write deadlines, 16 KiB headers, 4 KiB action
  bodies, 8 MiB responses. Transfer-Encoding, repeated headers, invalid lengths,
  non-object action JSON and unsafe state paths fail closed.
- Note reads are capped at 1 MiB, indexing at 10,000 notes / 64 MiB input; indexed
  text remains the existing first 60,000 characters. Linked note/state ancestors
  are refused, including internal symlinks previously accepted by Python. Note
  API paths must be relative and have no leading dot component. Root-level and
  custom-folder Markdown remain readable, independently of the CLI/sync kind
  allowlist.

`watch-brain` remains a compatibility readiness supervisor and can restart an
idle standalone worker at its existing three-second poll. Service residency is
therefore preserved until daemon activation is implemented separately in N0030.
Window rules themselves do not require a resident supervisor. No unit template,
activation policy or live service is changed here.

## Verification (2026-10-22)

Code checkpoint: `36aae6d2`. The complete desktop job from
`.github/workflows/check.yml` passed in the task's Arch container on dev-1,
including `cargo fmt --check`, strict Clippy, examples, all Rust suites (475
helper library tests), the minimal protocol consumer, release build,
`tools/check.py`, and the English/pseudo, AUR and wide-font galleries. The
container used SYS_ADMIN, unconfined seccomp/AppArmor and `/dev/fuse`; all builds
used nice 10 and Cargo `-j2`. The source suites passed 452 tools tests (one
existing skip), 120 AI tests, 45 Brain tests and 479 shell-tool tests; all 534
frozen helper cases passed without regenerating references. Gallery audits
passed 244 states per language and 185 states per language with the wide font.

Additional fixture-only checks:

- `run.py --case '<name>/*' --command 'nacre-helper {name}'` replayed all six
  frozen cases across brain-control/auth/discovery/launch.
- `python brain/tests/verify_native.py --native target/release/nacre-helper`
  passed 23 exact discovery comparisons and the public graph comparison.
- `python brain/tests/verify_semantic_native.py target/release/nacre-helper`
  passed optional-worker batching/cache/fallback and ephemeral-port idle exit.
- `brain/tests/browser-native.mjs` exercised the served UI with a private
  headless Chrome profile on the desktop under its heavy-job lock: graph,
  search, reader, root Markdown, assignment, bootstrap replay, cross-port
  navigation, stolen-cookie replay, fetch input origin isolation, invalid
  fragment rejection and restart/handoff. No real vault or live service was used.
  The final CI binary passed this repeat with zero browser exceptions; its
  SHA-256 is `1e09a21d4474f1ee103dda32b76f2aec389817d6cbb3fed0e085e831edab7099`.

CI also required the existing test-only sidebar timing repair from `bde78b50`
(cherry-picked as `aeb3105a`): wait for the released fake provider under its
45-second alarm instead of an eight-second scheduling deadline. Sidebar
production code is unchanged. No push, publication or live installation.
