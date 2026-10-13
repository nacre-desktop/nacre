"""Native raw-value template adapter used by the desktop publisher."""

from ._backend import call


def render_values(template, values, schemes=None, image=None, custom=None, data=None):
    return call(
        "render_values", template=template, values=values, schemes=schemes, image=image, custom=custom, data=data
    )
