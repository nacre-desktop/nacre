Status: blocked WIP; producer Bead CI and Nacre CI were not run, so this is not integration-ready.
Code commit: 980c71713 (WIP; [no-publish]); branch: task/codex-n0123-edge-hover-20261104.
Change: Nacre now parses Approach/Hover progress and sends output/edge/position/phase cues through Bead's external cue channel.
Capability gate requires compositor edge_trigger_cues plus the acknowledged event stream; missing capability preserves the fallback.
Helper normalization/projection now preserves cue events and cue capability; EOF and output removal send Left releases.
Click-only preference immediately returns edge input to the native QML/Bead handle path.
Visual strategy: left/right keep existing artwork, while controller input regions omit those handles under native takeover.
Top keeps the interactive bar header for controls; its separate lip input is disabled and cue hover drives header state.
Bottom has no Nacre handle; no trigger-length is generated for bottom and unmatched events are ignored by lip hit testing.
Generated nacre-niri config enables cues, uses configured edge thickness, and sets left/right length 144 and top length 208.
Added Nacre channel-delivery/lifecycle, capability-gate, QML state/mask, config, and scale 1/1.5 SVG pixel tests.
`git diff --check` and staged `git diff --cached --check`: exit 0; no build/test was run on this laptop.
Bead candidate pin fa28475171d708a1b1b4793a1876f5e910200e90 is producer-CI-unvalidated; exact manifest/lock/guard patch is unapplied at .lead/n0123-bead-pin.patch.
CI is blocked by dev-1 Docker overlay2 container creation: original PID 2888401 remains pending; no container/CI link was produced.
My duplicate PID 2965783 was terminated (exit 143); a later SSH check was interrupted locally (exit 130); host cleanup remains blocked.
No pixel measurements were collected; scale assertions are test code only. Product diff: 571 insertions, 303 deletions across 17 files.
