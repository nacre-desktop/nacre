# N0407 — Package dependency review after the QML shell's retirement

Status: planned (release lead, 2026-11-11). Starts after N0406 lands, because both edit
`packaging/packages.json` and the generated PKGBUILD.

The `nacre` core package hard-depends on 58 packages, which a fresh v0.1 install pulls
in whether or not the user needs them. Several date from the QML shell or from the
owner's personal setup. Examples: `qt6-declarative`, `qt6ct`, `breeze`, `kconfig`,
`kcolorscheme`, `dolphin` with `kio-extras`, `rofi`, `fish`, `kitty`, `fastfetch`,
`hypridle`/`hyprlock` on niri, `kwallet-pam`, `polkit-gnome`, `libgweather-4` and
`geoclue`. Source checks still install Qt for the Dolphin style plugin and the SDDM
greeter.

Do this:
1. Make a table with one row per `core` dependency: which installed file or route uses
   it (file:line on main), what breaks without it, and the verdict: **keep** (needed
   for the core shell to start and work), **optdepend** (an app or integration that
   already handles its absence, such as a missing executable or a hidden item), **move**
   (belongs to a session, AI, Brain or greeter package), or **drop** (nothing uses it).
2. Apply the verdicts in `packaging/packages.json` and the recipe, with optdepends
   descriptions in plain words. Where a feature crashes or shows dead UI when its
   dependency is missing, fix it or keep the dependency. Don't add new fallback code
   to justify an optdepend. If the Shell lead's N0206 (optional extras) covers a case,
   reuse its pattern.
3. `fish`, `kitty`, `fastfetch` and `dolphin` are Nacre's default apps, not shell
   requirements. Recommend keep or optdepend for each in the report; the owner decides
   what v0.1 ships as defaults.

Acceptance: the N0401 VM gate passes on all three tiers with the reduced set; the full
dev-1 desktop CI and the package proofs (`test-installed.py` for each session) pass;
the report lists the before and after dependency counts and install size
(`pacman -Qi` installed size for the closure in a clean container).

Not covered: the dependencies of the AI and Brain packages, the build and check lists
(CI-only), and the SDDM greeter's own packaging.
