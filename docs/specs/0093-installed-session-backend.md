# N0093: Installed Niri session backend

Status: Done; full Arch desktop CI replica passed on dev-2.

`install.py` and `nacre session refresh` must both pass the backend installed
with the shell tools. The backend lives under
`~/.local/share/nacre/shell/tools/backend`; it links the installed shell and
tools and contains the remaining inputs needed to prepare the private Niri
session. It carries the installed source revision so refresh does not need the
deploy checkout or its Git metadata. Package installs continue to use the
package backend.

The regression test deploys backend inputs from a temporary checkout, removes
that checkout, then runs the real helper through the owner-facing refresh path
under a temporary HOME. It checks regenerated XF86AudioRaiseVolume and
XF86AudioLowerVolume key-step binds with a non-overriding `custom.kdl`; the
helper's `niri validate` is the configuration validity gate. Refresh preserves
the prepared session's stock-Niri mode. The full desktop CI job passed for the
implementation, including all source/native checks and the daemon memory
regression test.

No live HOME/session operation or public release is part of this unit. No
temporary duplicate implementation paths were introduced.
