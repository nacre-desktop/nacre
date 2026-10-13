"""Compatibility API for the Rust sRGB/OKLab calculations."""

from ._backend import call


def clean(value):
    return call("clean", value=value)


def rgb(value):
    return tuple(call("rgb", value=value))


def linear(channel):
    return call("linear", channel=channel)


def encoded(channel):
    return call("encoded", channel=channel)


def luminance(value):
    return call("luminance", value=value)


def contrast(a, b):
    return call("contrast", a=a, b=b)


def lab_from_rgb(channels):
    return tuple(call("lab_from_rgb", channels=list(channels)))


def lch(value):
    return tuple(call("lch", value=value))


def lab_to_linear(light, a, b):
    return tuple(call("lab_to_linear", light=light, a=a, b=b))


def color(light, chroma, hue):
    return call("color", light=light, chroma=chroma, hue=hue)


def foreground(background):
    return call("foreground", background=background)


def readable(value, backgrounds, minimum=4.5):
    return call("readable", value=value, backgrounds=list(backgrounds), minimum=minimum)


def hue_distance(a, b):
    return call("hue_distance", a=a, b=b)
