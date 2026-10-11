# Nacre boot login

This is an original Qt6 SDDM theme matching the desktop lock screen. Authentication, session startup and password-keyring integration stay with SDDM/PAM; automatic login is not enabled.

## Install

Publish the current wallpaper/colors (this also updates installed login appearance):

```sh
python3 nacre/shell-tools/login-appearance.py
```

To preview without authenticating or changing the active SDDM session, use a
private copy of Main.qml/Logo.qml/metadata.desktop/theme.conf, with the generated theme.conf as theme.conf.user. Run sddm-greeter-qt6 --test-mode --theme PATH. Test mode cannot authenticate or perform power actions.

Install the reviewed source with administrator authentication:

```sh
pkexec /usr/bin/python3 nacre/shell-tools/login-install.py --user "$USER"
python3 nacre/shell-tools/login-appearance.py
```

It writes /etc/sddm.conf.d/90-nacre-theme.conf and a root-owned /usr/share/sddm/themes/nacre. It does not restart SDDM; the change applies on the next boot/login. Previous theme/override files are backed up under /var/lib/nacre/login/backups with a rollback manifest. Removing the added override restores the previous default when no earlier override existed.

## Wallpaper integration

The only user-writable system data is /var/lib/nacre/login/appearance: a generated blurred PNG and an INI with validated palette colors. The root-owned theme.conf.user points to that INI. No user QML is executed from this folder, and no privileged wallpaper watcher/service is needed. classic-state.py publishes these files when the desktop commits a wallpaper/color scheme. Pillow renders the image; image work is cached across theme-mode changes. Missing rendering dependencies leave the desktop palette working and the previous login appearance intact.

Tests: python3 -m unittest discover -s nacre/shell-tools/tests and QT_QPA_PLATFORM=offscreen /usr/lib/qt6/bin/qmltestrunner -input nacre/login/tests. The harness uses fake authentication; successful real password login must be checked at the next login.

The shared Logo.qml and LogoData.js derive from nacre/shell/branding/nacre-master.svg. Run python3 nacre/shell-tools/branding.py --build after editing that approved master. The same shell geometry feeds web, Qt, Kitty and lock outputs, with primary/secondary/tertiary chamber roles and a pearl highlight. The root-owned theme includes both generated files; changing source never restarts SDDM or changes authentication.
