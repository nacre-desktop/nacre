"""Thin API for the Rust export-only color namespaces."""

from ._backend import call

BASE16_ROLES = tuple(call("BASE16_ROLES"))


def base16_contexts(contexts, active_mode):
    return call("base16_contexts", contexts=contexts, active_mode=active_mode)


def tonal_contexts(values):
    return call("tonal_contexts", values=values)
