# Software Phase 2 fixture gallery

Theme captures show both modes and wallpapers before Apply, using the production
launcher components with synthetic providers. They are nested Qt captures, not
laptop hardware verification. Regenerate with:

```sh
python tools/i18n/audit.py --download-fonts --output /tmp/nacre-theme-gallery themes
```

The CI native-gallery artifact includes English and pseudo-language captures.
Wallpaper provenance and licenses are in `nacre/shell-tools/theme-bundles/`.
