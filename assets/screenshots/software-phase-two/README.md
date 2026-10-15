# Software Phase 2 fixture gallery

Theme captures show both modes and wallpapers before Apply, using the production
launcher components with synthetic providers. They are nested Qt captures, not
laptop hardware verification. Regenerate with:

```sh
python tools/i18n/audit.py --download-fonts --output /tmp/nacre-theme-gallery themes
```

The CI native-gallery artifact includes English and pseudo-language captures.
Wallpaper provenance and licenses are in `nacre/shell-tools/theme-bundles/`.

AUR captures show real Kitty/less on an authenticated private Xvfb display, using
inert local recipes through the production read-only inspector. They include
the complete-recipe view and a changed-recipe diff. Regenerate with
`python tools/capture-aur-review.py --output /tmp/nacre-aur-gallery`.
The gallery performs no build, install or live desktop capture.
