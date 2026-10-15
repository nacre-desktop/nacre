"""Temporary native-only renderer client for helpers retired in later waves."""

from nacre_native_palette import _call


def render_values(template, values, schemes=None, image=None, custom=None, data=None):
    return _call(
        "palette_engine",
        "render_values",
        template,
        values,
        schemes=schemes,
        image=image,
        custom=custom,
        data=data,
    )
