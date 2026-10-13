"""Native decoding/analysis adapter."""

from ._backend import call


def analyze(path):
    return call("analyze", path=str(path))


def sample_color(path, x, y):
    return call("sample_color", path=str(path), x=x, y=y)
