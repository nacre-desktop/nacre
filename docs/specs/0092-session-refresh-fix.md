# N0092: Niri session refresh fix

The deploy and `nacre session refresh` commands now use one transactional
session refresh implementation. It keeps a rollback copy, prepares the config
from the active backend tree, reloads Niri through the compositor module/API,
and restores the previous session in place on failure. Repo installs resolve
the backend root recorded by `install.py` in the installed tools tree; package
installs retain `/usr/share/nacre`.

The installer crash came from calling the imported `nacre_native_compositor`
module as a function. The CLI failure came from always selecting the package
backend path. Regression tests cover reload, rollback, backend resolution and
controller installation. No temporary duplicate paths were introduced.
