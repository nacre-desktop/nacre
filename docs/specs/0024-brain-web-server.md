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
