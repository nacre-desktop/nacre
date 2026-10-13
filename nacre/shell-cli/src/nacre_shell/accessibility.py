"""Compatibility API for Rust role audits and color-vision simulations."""

from ._backend import call


def simulate(value, vision="normal"):
    return call("simulate", value=value, vision=vision)


def difference(a, b, vision="normal"):
    return call("difference", a=a, b=b, vision=vision)


def text_pairs(colors):
    return [tuple(pair) for pair in call("text_pairs")]


def audit(colors):
    return call("audit", colors=colors)
