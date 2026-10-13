"""Desktop engine API delegated to the pinned native executable."""

from ._backend import call


def from_image(
    path,
    mode="dark",
    variant="tonalspot",
    flavour="default",
    accent=None,
    smart=False,
    harmony=False,
    personality=None,
    overrides=None,
    brightness=0.0,
    saturation=1.0,
    hour=12,
    cache_dir=None,
    background_from_wallpaper=None,
    time_tint=False,
):
    options = dict(locals())
    options.pop("path")
    if cache_dir is not None:
        options["cache_dir"] = str(cache_dir)
    return call("from_image", path=str(path), options=options)


def default_palette(mode="dark", variant="tonalspot", flavour="default"):
    return call("default_palette", options=dict(mode=mode, variant=variant, flavour=flavour))
