# N0024 — Brain web server as an on-demand worker

- Status: In progress
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
  same-origin fetches only. No credential appears in a static asset or cookie.
  Protected API requests require both credentials; cross-port navigation and a
  stolen-cookie replay with forged Fetch Metadata must fail.
- Protected GET APIs additionally require `Sec-Fetch-Site: same-origin` and
  reject foreign Origin/Referer. Non-browser clients must supply that metadata
  and the private API credential. POST retains exact canonical Origin, cookie
  and application/json checks. The canonical origin remains `127.0.0.1`.
- Bootstrap expires after five minutes and is consumed once. Explicit launches
  refresh its private handoff file. Restart revokes old bootstraps. Sessions
  expire after 30 days server-side and via cookie Max-Age; each listener port
  has separate session/marker state under `auth/ports` (this does not port-scope
  browser cookies). Legacy sessions reauthenticate through the existing handoff.
- At most 32 clients; absolute 60-second connection lifetime, five-second
  request parse/body and response-write deadlines, 16 KiB headers, 4 KiB action
  bodies, 8 MiB responses. Transfer-Encoding, repeated headers, invalid lengths,
  non-object action JSON and unsafe state paths fail closed.
- Note reads are capped at 1 MiB, indexing at 10,000 notes / 64 MiB input; indexed
  text remains the existing first 60,000 characters. Linked note/state ancestors
  are refused, including internal symlinks previously accepted by Python.

`watch-brain` remains a compatibility readiness supervisor and can restart an
idle standalone worker at its existing three-second poll. Service residency is
therefore preserved until daemon activation is implemented separately in N0030.
Window rules themselves do not require a resident supervisor. No unit template,
activation policy or live service is changed here.
