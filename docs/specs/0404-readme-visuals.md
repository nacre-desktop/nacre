# N0404 — README visuals, phase 1

Phase 1 adds README visuals to Bead, Orient, and nacre-niri. Nacre’s own README remains for phase 2, near the v0.1 cut after the Shell lead’s visible changes land.

| Repo | Captured visual | Method |
| --- | --- | --- |
| Bead | `docs/images/minimal-shell.png` — 1920×1080, 503,571 bytes | Built `examples/minimal-shell` in a capped Ubuntu 24.04 container; ran it in a fresh private Arch overlay under virgl/Niri at scale 1 with Nacre’s coast demo wallpaper. The example is static, so no GIF. |
| Orient | `docs/images/orient-wallpaper-palette.png` — 1600×900, 562,986 bytes | Paired Nacre’s forest demo wallpaper with its semantic palette from the Orient CLI (`orient palette input/forest.jpg --personality natural --mode dark`); rendered by the uncommitted `render.py`. |
| nacre-niri | `docs/images/nacre-niri-overview.png` — 1920×1080, 107,102 bytes; `docs/images/nacre-niri-tabbed-column.png` — 1920×1080, 322,385 bytes | Built fork revision `a8562a11` in a capped Arch container; captured its overview with the focused title highlighted and a tabbed Foot column in a fresh private virgl Arch overlay at scale 1, using Nacre’s aurora demo wallpaper. |

All VM captures used tty1 autologin, neutral bundled wallpapers, and no browser tabs or personal names; PNGs are below 600,000 bytes. Capture and render scripts remain uncommitted in their worktrees’ `.lead/` directories. Rerun with Nacre’s `.lead/n0404/nacre-niri-capture.sh`, Bead’s `.lead/capture-bead-minimal-shell.sh`, and Orient’s `.lead/n0404/render.py` from their respective worktrees.
