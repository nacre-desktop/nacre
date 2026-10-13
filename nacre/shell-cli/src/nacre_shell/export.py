"""Native theme rendering/export adapter."""

from ._backend import call


def render(template, palette, schemes=None, custom=None):
    return call("render", template=template, palette=palette, schemes=schemes, custom=custom)


def export(palette, directory, templates=None, schemes=None, custom=None):
    return call(
        "export",
        palette=palette,
        directory=str(directory),
        templates=str(templates) if templates else None,
        schemes=schemes,
        custom=custom,
    )
